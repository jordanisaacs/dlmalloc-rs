extern crate rustix;

use self::rustix::mm::{self, MapFlags, MremapFlags, ProtFlags};
use core::ptr;
use Allocator;

/// System setting for Linux
pub struct System {
    _priv: (),
}

impl System {
    pub const fn new() -> System {
        System { _priv: () }
    }
}

#[cfg(feature = "global")]
static mut LOCK: c_scape::pthread_mutex_t = c_scape::PTHREAD_MUTEX_INITIALIZER;

unsafe impl Allocator for System {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        let addr = unsafe {
            rustix::mm::mmap_anonymous(
                0 as *mut _,
                size,
                ProtFlags::WRITE | ProtFlags::READ,
                MapFlags::PRIVATE,
            )
        };

        if let Ok(addr) = addr {
            (addr as *mut u8, size, 0)
        } else {
            (ptr::null_mut(), 0, 0)
        }
    }

    #[cfg(target_os = "linux")]
    fn remap(&self, ptr: *mut u8, oldsize: usize, newsize: usize, can_move: bool) -> *mut u8 {
        let flags = if can_move {
            MremapFlags::MAYMOVE
        } else {
            MremapFlags::empty()
        };
        let ptr = unsafe { mm::mremap(ptr as *mut _, oldsize, newsize, flags) };
        if let Ok(ptr) = ptr {
            ptr as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    #[cfg(target_os = "macos")]
    fn remap(&self, _ptr: *mut u8, _oldsize: usize, _newsize: usize, _can_move: bool) -> *mut u8 {
        ptr::null_mut()
    }

    #[cfg(target_os = "linux")]
    fn free_part(&self, ptr: *mut u8, oldsize: usize, newsize: usize) -> bool {
        unsafe {
            let rc = mm::mremap(ptr as *mut _, oldsize, newsize, MremapFlags::empty());
            if rc.is_ok() {
                return true;
            }
            mm::munmap(ptr.offset(newsize as isize) as *mut _, oldsize - newsize).is_ok()
        }
    }

    #[cfg(target_os = "macos")]
    fn free_part(&self, ptr: *mut u8, oldsize: usize, newsize: usize) -> bool {
        unsafe { mm::munmap(ptr.offset(newsize as isize) as *mut _, oldsize - newsize).is_ok() }
    }

    fn free(&self, ptr: *mut u8, size: usize) -> bool {
        unsafe { mm::munmap(ptr as *mut _, size).is_ok() }
    }

    fn can_release_part(&self, _flags: u32) -> bool {
        true
    }

    fn allocates_zeros(&self) -> bool {
        true
    }

    fn page_size(&self) -> usize {
        4096
    }
}

#[cfg(feature = "global")]
pub fn acquire_global_lock() {
    unsafe { assert_eq!(c_scape::pthread_mutex_lock(&mut LOCK), 0) }
}

#[cfg(feature = "global")]
pub fn release_global_lock() {
    unsafe { assert_eq!(c_scape::pthread_mutex_unlock(&mut LOCK), 0) }
}

#[cfg(feature = "global")]
/// allows the allocator to remain unsable in the child process,
/// after a call to `fork(2)`
///
/// #Safety
///
/// if used, this function must be called,
/// before any allocations are made with the global allocator.
pub unsafe fn enable_alloc_after_fork() {
    // atfork must only be called once, to avoid a deadlock,
    // where the handler attempts to acquire the global lock twice
    static mut FORK_PROTECTED: bool = false;

    unsafe extern "C" fn _acquire_global_lock() {
        acquire_global_lock()
    }

    unsafe extern "C" fn _release_global_lock() {
        release_global_lock()
    }

    acquire_global_lock();
    // if a process forks,
    // it will acquire the lock before any other thread,
    // protecting it from deadlock,
    // due to the child being created with only the calling thread.
    if !FORK_PROTECTED {
        c_scape::pthread_atfork(
            Some(_acquire_global_lock),
            Some(_release_global_lock),
            Some(_release_global_lock),
        );
        FORK_PROTECTED = true;
    }
    release_global_lock();
}
