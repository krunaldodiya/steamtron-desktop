//! Track a launched game's whole process tree.
//!
//! Many launchers start the real game and exit (Witcher 3's REDprelauncher,
//! RDR2's Razor1911 `Launcher.exe`). A Windows job object follows every
//! descendant, so the overlay stays mounted and the Play button stays Stop
//! until the last game process exits.
//!
//! The job also kills the tree when the wrapper or the app's Stop button closes
//! the job, matching the intended direct-child lifetime.

#![cfg(windows)]

use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
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

    /// Kill every process in the job.
    pub fn terminate(&self) -> bool {
        unsafe { TerminateJobObject(self.job, 1) != 0 }
    }
}

// A job handle is a kernel object handle; the Win32 APIs used here accept it
// from any thread. The app stores one in the shared run map so Stop can
// terminate the tree from a worker thread.
unsafe impl Send for ProcessTree {}
unsafe impl Sync for ProcessTree {}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        if !self.job.is_null() {
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}
