//! Keep the fixes overlay mounted for the whole game process tree.
//!
//! Many fixes ship a launcher that starts the real game and exits (RDR2's
//! Razor1911 `Launcher.exe` is one). Waiting only for that first process tore
//! the union down while the game was still loading, which looked like a crash
//! halfway through the intro. A Windows job object follows every descendant,
//! so the mount lives until the last game process exits.
//!
//! The job also kills the tree when the wrapper is stopped, matching the old
//! direct-child lifetime for the Stop button.

#![cfg(windows)]

use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct ProcessTree {
    job: HANDLE,
    assigned: bool,
}

impl ProcessTree {
    pub fn new() -> Option<Self> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // Not fatal: if limits cannot be set the job still tracks members.
            let _ = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            Some(Self {
                job,
                assigned: false,
            })
        }
    }

    /// Attach the launched process so its children inherit the job.
    pub fn assign(&mut self, child: &std::process::Child) -> bool {
        let handle = child.as_raw_handle() as HANDLE;
        let assigned = unsafe { AssignProcessToJobObject(self.job, handle) != 0 };
        self.assigned = assigned;
        assigned
    }

    /// Processes still alive in the job (zero when the tree is done).
    pub fn active(&self) -> u32 {
        if !self.assigned {
            return 0;
        }
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            );
            if ok != 0 {
                info.ActiveProcesses
            } else {
                0
            }
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        if !self.job.is_null() {
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}
