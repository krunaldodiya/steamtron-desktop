//! Windows backend: a WinFsp union filesystem with copy-up into the
//! persistent per-game upper layer.

use std::collections::BTreeMap;
use std::ffi::{OsString, c_void};
use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::os::windows::fs::FileExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use winfsp::filesystem::{
    DirBuffer, DirInfo, DirMarker, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo,
    VolumeInfo, WideNameInfo,
};
use winfsp::host::{CoarseGuard, FileSystemHost, VolumeParams};
use winfsp::{FspInit, Result as FspResult, U16CStr, winfsp_init};

use crate::{OverlayError, OverlayGame, OverlayLayout, default_data_home};

const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x0000_0020;
const FILE_WRITE_DATA: u32 = 0x0000_0002;
const FILE_APPEND_DATA: u32 = 0x0000_0004;
const FILE_WRITE_EA: u32 = 0x0000_0010;
const FILE_WRITE_ATTRIBUTES: u32 = 0x0000_0100;
const DELETE: u32 = 0x0001_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

fn wants_write(access: u32) -> bool {
    access
        & (FILE_WRITE_DATA
            | FILE_APPEND_DATA
            | FILE_WRITE_EA
            | FILE_WRITE_ATTRIBUTES
            | DELETE
            | WRITE_DAC
            | WRITE_OWNER
            | GENERIC_WRITE)
        != 0
}

fn filetime(time: std::time::SystemTime) -> u64 {
    const WINDOWS_EPOCH: u64 = 116_444_736_000_000_000;
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64 / 100 + WINDOWS_EPOCH)
        .unwrap_or(WINDOWS_EPOCH)
}

fn info_for(metadata: &fs::Metadata) -> FileInfo {
    let mut attributes = if metadata.is_dir() {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_ARCHIVE
    };
    if metadata.permissions().readonly() {
        attributes |= FILE_ATTRIBUTE_READONLY;
    }
    FileInfo {
        file_attributes: attributes,
        allocation_size: metadata.len(),
        file_size: metadata.len(),
        creation_time: metadata.created().map(filetime).unwrap_or(0),
        last_access_time: metadata.accessed().map(filetime).unwrap_or(0),
        last_write_time: metadata.modified().map(filetime).unwrap_or(0),
        change_time: metadata.modified().map(filetime).unwrap_or(0),
        ..FileInfo::default()
    }
}

fn relative(file_name: &U16CStr) -> Result<PathBuf, std::io::Error> {
    let text = file_name.to_string_lossy();
    let mut result = PathBuf::new();
    let path = Path::new(text.trim_start_matches(['\\', '/']));
    if path
        .components()
        .next()
        .is_some_and(|value| value.as_os_str() == ".steamtron-whiteouts")
    {
        return Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "reserved overlay path",
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(value) => result.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(std::io::Error::new(
                    ErrorKind::PermissionDenied,
                    "path escapes the overlay root",
                ));
            }
        }
    }
    Ok(result)
}

struct UnionFs {
    common: PathBuf,
    fixes: PathBuf,
    upper: PathBuf,
}

struct UnionFile {
    path: PathBuf,
    relative: PathBuf,
    file: Option<File>,
    is_dir: bool,
    delete_on_cleanup: AtomicBool,
}

impl UnionFs {
    fn candidate(&self, root: &Path, relative: &Path) -> PathBuf {
        root.join(relative)
    }

    fn resolve(&self, relative: &Path) -> Option<(PathBuf, bool)> {
        if self.is_whiteout(relative) {
            return None;
        }
        let upper = self.candidate(&self.upper, relative);
        if upper.exists() {
            return Some((upper, true));
        }
        let fixes = self.candidate(&self.fixes, relative);
        if fixes.exists() {
            return Some((fixes, false));
        }
        let common = self.candidate(&self.common, relative);
        if common.exists() {
            return Some((common, false));
        }
        None
    }

    fn upper_path(&self, relative: &Path) -> PathBuf {
        self.candidate(&self.upper, relative)
    }

    fn whiteout_path(&self, relative: &Path) -> PathBuf {
        self.upper.join(".steamtron-whiteouts").join(relative)
    }

    fn is_whiteout(&self, relative: &Path) -> bool {
        self.whiteout_path(relative).is_file()
    }

    fn set_whiteout(&self, relative: &Path) -> std::io::Result<()> {
        let marker = self.whiteout_path(relative);
        if let Some(parent) = marker.parent() {
            fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(marker)?;
        Ok(())
    }

    fn clear_whiteout(&self, relative: &Path) {
        let marker = self.whiteout_path(relative);
        let _ = fs::remove_file(marker);
    }

    fn copy_up(&self, relative: &Path, source: &Path) -> std::io::Result<PathBuf> {
        let target = self.upper_path(relative);
        if target.exists() {
            return Ok(target);
        }
        self.clear_whiteout(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if source.is_dir() {
            fs::create_dir_all(&target)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let child = entry.file_name();
                let child_relative = relative.join(&child);
                let child_source = source.join(&child);
                self.copy_up(&child_relative, &child_source)?;
            }
        } else {
            fs::copy(source, &target)?;
        }
        Ok(target)
    }

    fn open_union_file(
        &self,
        relative: &Path,
        access: u32,
        create: bool,
    ) -> std::io::Result<UnionFile> {
        let (path, upper) = match self.resolve(relative) {
            Some(value) => value,
            None if create => {
                self.clear_whiteout(relative);
                (self.upper_path(relative), true)
            }
            None => return Err(std::io::Error::from(ErrorKind::NotFound)),
        };
        let metadata = fs::metadata(&path)?;
        let is_dir = metadata.is_dir();
        let path = if !is_dir && wants_write(access) && !upper {
            self.copy_up(relative, &path)?
        } else {
            path
        };
        let file = if is_dir {
            None
        } else {
            let mut options = OpenOptions::new();
            options.read(true).write(wants_write(access));
            Some(options.open(&path)?)
        };
        Ok(UnionFile {
            path,
            relative: relative.to_path_buf(),
            file,
            is_dir,
            delete_on_cleanup: AtomicBool::new(false),
        })
    }

    fn merged_directory(&self, relative: &Path) -> std::io::Result<Vec<DirEntry>> {
        let mut entries: BTreeMap<OsString, PathBuf> = BTreeMap::new();
        for root in [&self.common, &self.fixes, &self.upper] {
            let directory = self.candidate(root, relative);
            let Ok(read_dir) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in read_dir {
                let entry = entry?;
                if root == &self.upper && entry.file_name() == ".steamtron-whiteouts" {
                    continue;
                }
                if self.is_whiteout(&relative.join(entry.file_name())) {
                    continue;
                }
                entries.insert(entry.file_name(), entry.path());
            }
        }
        Ok(entries
            .into_iter()
            .map(|(name, path)| DirEntry { name, path })
            .collect())
    }
}

struct DirEntry {
    name: OsString,
    path: PathBuf,
}

impl FileSystemContext for UnionFs {
    type FileContext = UnionFile;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> FspResult<FileSecurity> {
        let relative = relative(file_name)?;
        let Some((path, _)) = self.resolve(&relative) else {
            return Err(std::io::Error::from(ErrorKind::NotFound).into());
        };
        let metadata = fs::metadata(path)?;
        Ok(FileSecurity {
            reparse: false,
            sz_security_descriptor: 0,
            attributes: info_for(&metadata).file_attributes,
        })
    }

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        granted_access: u32,
        file_info: &mut OpenFileInfo,
    ) -> FspResult<Self::FileContext> {
        let relative = relative(file_name)?;
        let context = self.open_union_file(&relative, granted_access, false)?;
        *file_info.as_mut() = info_for(&fs::metadata(&context.path)?);
        Ok(context)
    }

    fn close(&self, _context: Self::FileContext) {}

    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        _granted_access: u32,
        file_attributes: u32,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> FspResult<Self::FileContext> {
        let relative = relative(file_name)?;
        let path = self.upper_path(&relative);
        let is_dir =
            file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 || create_options & 0x0000_0001 != 0;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if is_dir {
            fs::create_dir_all(&path)?;
        } else {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)?;
        }
        let context = self.open_union_file(&relative, 0, true)?;
        *file_info.as_mut() = info_for(&fs::metadata(&context.path)?);
        Ok(context)
    }

    fn cleanup(&self, context: &Self::FileContext, _file_name: Option<&U16CStr>, _flags: u32) {
        if context.delete_on_cleanup.load(Ordering::SeqCst) {
            if context.relative.as_os_str().is_empty() {
                return;
            }
            let had_lower = self.candidate(&self.fixes, &context.relative).exists()
                || self.candidate(&self.common, &context.relative).exists();
            let _ = if context.is_dir {
                fs::remove_dir_all(&context.path)
            } else {
                fs::remove_file(&context.path)
            };
            if had_lower {
                let _ = self.set_whiteout(&context.relative);
            }
        }
    }

    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        file_info: &mut FileInfo,
    ) -> FspResult<()> {
        let Some(context) = context else {
            return Ok(());
        };
        if let Some(file) = context.file.as_ref() {
            file.sync_all()?;
        }
        *file_info = info_for(&fs::metadata(&context.path)?);
        Ok(())
    }

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut FileInfo,
    ) -> FspResult<()> {
        *file_info = info_for(&fs::metadata(&context.path)?);
        Ok(())
    }

    fn overwrite(
        &self,
        context: &Self::FileContext,
        _file_attributes: u32,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        file_info: &mut FileInfo,
    ) -> FspResult<()> {
        if let Some(file) = context.file.as_ref() {
            file.set_len(0)?;
        }
        *file_info = info_for(&fs::metadata(&context.path)?);
        Ok(())
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker,
        buffer: &mut [u8],
    ) -> FspResult<u32> {
        if !context.is_dir {
            return Err(std::io::Error::from(ErrorKind::NotADirectory).into());
        }
        let relative = context
            .path
            .strip_prefix(&self.upper)
            .or_else(|_| context.path.strip_prefix(&self.fixes))
            .or_else(|_| context.path.strip_prefix(&self.common))
            .map_err(|_| std::io::Error::from(ErrorKind::NotFound))?;
        let entries = self.merged_directory(relative)?;
        let directory_buffer = DirBuffer::new();
        let lock = directory_buffer.acquire(marker.is_none(), Some(entries.len() as u32 + 1))?;
        let mut skipping = marker.inner_as_cstr().map(|value| value.to_os_string());
        for entry in entries {
            if let Some(marker_name) = skipping.as_ref() {
                if entry.name == *marker_name {
                    skipping = None;
                }
                continue;
            }
            let mut info = DirInfo::<255>::new();
            let metadata = fs::metadata(&entry.path)?;
            *info.file_info_mut() = info_for(&metadata);
            info.set_name(&entry.name)?;
            let _ = lock.write(&mut info);
        }
        drop(lock);
        Ok(directory_buffer.read(marker, buffer))
    }

    fn rename(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> FspResult<()> {
        let relative = relative(new_file_name)?;
        let source_is_upper = context.path.starts_with(&self.upper);
        let source = if source_is_upper {
            context.path.clone()
        } else {
            self.copy_up(&context.relative, &context.path)?
        };
        let target = self.upper_path(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if replace_if_exists && target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        if source.exists() {
            fs::rename(&source, &target)?;
            self.clear_whiteout(&context.relative);
        }
        Ok(())
    }

    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        file_attributes: u32,
        _creation_time: u64,
        last_access_time: u64,
        last_write_time: u64,
        _last_change_time: u64,
        file_info: &mut FileInfo,
    ) -> FspResult<()> {
        let path = if context.path.starts_with(&self.upper) {
            context.path.clone()
        } else {
            self.copy_up(&context.relative, &context.path)?
        };
        let mut permissions = fs::metadata(&path)?.permissions();
        if file_attributes & FILE_ATTRIBUTE_READONLY != 0 {
            permissions.set_readonly(true);
        } else {
            permissions.set_readonly(false);
        }
        fs::set_permissions(&path, permissions)?;

        let access = windows_time(last_access_time);
        let write = windows_time(last_write_time);
        if !context.is_dir {
            let file = OpenOptions::new().write(true).open(&path)?;
            let times = fs::FileTimes::new()
                .set_accessed(access)
                .set_modified(write);
            file.set_times(times)?;
        }
        *file_info = info_for(&fs::metadata(&path)?);
        Ok(())
    }

    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> FspResult<()> {
        context
            .delete_on_cleanup
            .store(delete_file, Ordering::SeqCst);
        Ok(())
    }

    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> FspResult<()> {
        if let Some(file) = context.file.as_ref() {
            file.set_len(new_size)?;
        }
        *file_info = info_for(&fs::metadata(&context.path)?);
        Ok(())
    }

    fn read(&self, context: &Self::FileContext, buffer: &mut [u8], offset: u64) -> FspResult<u32> {
        let Some(file) = context.file.as_ref() else {
            return Err(std::io::Error::from(ErrorKind::IsADirectory).into());
        };
        Ok(file.seek_read(buffer, offset)? as u32)
    }

    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        _constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> FspResult<u32> {
        let Some(file) = context.file.as_ref() else {
            return Err(std::io::Error::from(ErrorKind::IsADirectory).into());
        };
        let offset = if write_to_eof {
            file.metadata()?.len()
        } else {
            offset
        };
        let written = file.seek_write(buffer, offset)?;
        *file_info = info_for(&fs::metadata(&context.path)?);
        Ok(written as u32)
    }

    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> FspResult<()> {
        out_volume_info.total_size = 1 << 40;
        out_volume_info.free_size = 1 << 39;
        out_volume_info.set_volume_label("SteamTron");
        Ok(())
    }
}

fn windows_time(value: u64) -> std::time::SystemTime {
    const WINDOWS_EPOCH: u64 = 116_444_736_000_000_000;
    if value < WINDOWS_EPOCH {
        return std::time::UNIX_EPOCH;
    }
    std::time::UNIX_EPOCH + std::time::Duration::from_nanos((value - WINDOWS_EPOCH) * 100)
}

pub struct WindowsOverlay {
    game: OverlayGame,
    layout: OverlayLayout,
    mount_point: PathBuf,
    host: Option<FileSystemHost<UnionFs, CoarseGuard>>,
    init: Option<FspInit>,
}

impl WindowsOverlay {
    pub fn new(game: OverlayGame) -> Self {
        Self::with_data_home(game, default_data_home())
    }

    pub fn with_data_home(game: OverlayGame, data_home: impl Into<PathBuf>) -> Self {
        let data_home = data_home.into();
        let layout = OverlayLayout::for_game(&game, &data_home);
        let mount_point = data_home.join("steamtron/mount").join(game.key());
        Self {
            game,
            layout,
            mount_point,
            host: None,
            init: None,
        }
    }

    pub fn game(&self) -> &OverlayGame {
        &self.game
    }

    pub fn layout(&self) -> &OverlayLayout {
        &self.layout
    }

    pub fn merged(&self) -> &Path {
        &self.mount_point
    }

    pub fn mount(&mut self) -> Result<&Path, OverlayError> {
        if self.host.is_some() {
            return Ok(self.merged());
        }
        self.layout.prepare()?;
        let init = winfsp_init().map_err(|error| OverlayError::MountFailed(error.to_string()))?;
        let fs = UnionFs {
            common: self.game.common().to_path_buf(),
            fixes: self.game.fixes().to_path_buf(),
            upper: self.layout.upper.clone(),
        };
        let params = {
            let mut params = VolumeParams::new();
            params.filesystem_name("SteamTron");
            params
        };
        fs::create_dir_all(&self.mount_point)
            .map_err(|error| OverlayError::io(&self.mount_point, error))?;
        let mut host = FileSystemHost::<UnionFs, CoarseGuard>::new(params, fs)
            .map_err(|error| OverlayError::MountFailed(error.to_string()))?;
        host.mount(&self.mount_point)
            .map_err(|error| OverlayError::MountFailed(error.to_string()))?;
        host.start()
            .map_err(|error| OverlayError::MountFailed(error.to_string()))?;
        self.host = Some(host);
        self.init = Some(init);
        Ok(self.merged())
    }

    pub fn unmount(&mut self) -> Result<(), OverlayError> {
        if let Some(mut host) = self.host.take() {
            host.stop();
            host.unmount();
        }
        self.init = None;
        let _ = fs::remove_dir(&self.mount_point);
        if self.layout.runtime.exists() {
            let _ = fs::remove_dir_all(&self.layout.runtime);
        }
        if self.layout.work.exists() {
            let _ = fs::remove_dir_all(&self.layout.work);
        }
        Ok(())
    }
}

impl Drop for WindowsOverlay {
    fn drop(&mut self) {
        let _ = self.unmount();
    }
}
