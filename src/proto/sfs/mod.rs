use {Status, Result, ucs2};
use core::mem;

bitflags! {
    pub struct FileMode : u64 {
        const READ      = 0x0000000000000001;
        const WRITE     = 0x0000000000000002;
        const CREATE    = 0x8000000000000000;
    }
}

bitflags! {
    pub struct FileAttribute : u64 {
        const NONE         = 0x0000000000000000;
        const READ_ONLY    = 0x0000000000000001;
        const HIDDEN       = 0x0000000000000002;
        const SYSTEM       = 0x0000000000000004;
        const RESERVED     = 0x0000000000000008;
        const DIRECTORY    = 0x0000000000000010;
        const ARCHIVE      = 0x0000000000000020;
        const VALID_ATTR   = 0x0000000000000037;
    }
}

#[repr(C)]
pub struct FileImpl {
    revision: u64,
    open: extern "C" fn(this: &mut FileImpl, new_handle: &mut usize, filename: *const u16, open_mode: FileMode, attributes: FileAttribute) -> Status,
    close: extern "C" fn(this: &mut FileImpl) -> Status,
    delete: extern "C" fn(this: &mut FileImpl) -> Status,
    read: extern "C" fn(this: &mut FileImpl, buffer_size: &mut usize, buffer: *mut u8) -> Status,
    write: extern "C" fn(this: &mut FileImpl, buffer_size: &mut usize, buffer: *const u8) -> Status,
    get_position: extern "C" fn(this: &mut FileImpl, position: &mut u64) -> Status,
    set_position: extern "C" fn(this: &mut FileImpl, position: u64) -> Status,
    get_info: usize,
    set_info: usize,
    flush: extern "C" fn(this: &mut FileImpl) -> Status,
}

#[repr(C)]
pub struct SimpleFileSystem {
    revision: u64,
    open_volume: extern "C" fn(this: &mut SimpleFileSystem, root: &mut usize) -> Status, 
}

pub struct File<'a> {
    inner: &'a mut FileImpl,
}

impl<'a> File<'a> {
    /// Try to open a file relative to this file/directory.
    ///
    /// # Arguments
    /// * `filename`    Path of file to open, relative to this File
    /// * `open_mode`   The mode to open the file with. Valid
    ///     combinations are READ, READ | WRITE and READ | WRITE | CREATE
    /// * `attributes`  Only valid when FILE_MODE_CREATE is used as a mode
    /// 
    /// # Errors
    /// * `uefi::Status::InvalidParameter`  The filename exceeds the maximum length of 255 chars
    /// * `uefi::Status::NotFound`          Could not find file
    /// * `uefi::Status::NoMedia`           The device has no media
    /// * `uefi::Status::MediaChanged`      The device has a different medium in it
    /// * `uefi::Status::DeviceError`       The device reported an error
    /// * `uefi::Status::VolumeCorrupted`   The filesystem structures are corrupted
    /// * `uefi::Status::WriteProtected`    Write/Create attempted on readonly file
    /// * `uefi::Status::AccessDenied`      The service denied access to the file
    /// * `uefi::Status::OutOfResources`    Not enough resources to open file
    /// * `uefi::Status::VolumeFull`        The volume is full
    pub fn open(&mut self, filename: &str, open_mode: FileMode, attributes: FileAttribute) -> Result<File> {
        const BUF_SIZE : usize = 255;
        if filename.len() > BUF_SIZE {
            Err(Status::InvalidParameter)
        }
        else {
            let mut buf = [0u16; BUF_SIZE+1];
            let mut ptr = 0usize;

            ucs2::encode_ucs2(filename, &mut buf)?;
            (self.inner.open)(self.inner, &mut ptr, buf.as_ptr(), open_mode, attributes).into_with(|| File {
                inner: unsafe { &mut *(ptr as *mut FileImpl) }
            })
        }
    }

    /// Close this file handle
    ///
    /// This MUST be called when you are done with the file
    pub fn close(self) -> Result<()> {
        (self.inner.close)(self.inner).into()
    }

    /// Closes and deletes this file
    ///
    /// # Errors
    /// * `uefi::Status::WarnDeleteFailure` The file was closed, but deletion failed
    pub fn delete(self) -> Result<()> {
        (self.inner.delete)(self.inner).into()
    }

    /// Read data from file
    ///
    /// Try to read as much as possible into `buffer`. Returns the number of bytes read
    ///
    /// # Arguments
    /// * `buffer`  The target buffer of the read operation
    ///
    /// # Errors
    /// * `uefi::Status::NoMedia`           The device has no media
    /// * `uefi::Status::DeviceError`       The device reported an error 
    /// * `uefi::Status::VolumeCorrupted`   The filesystem structures are corrupted
    pub fn read(&mut self, buffer: &mut[u8]) -> Result<usize> {
        let mut buffer_size = buffer.len();
        (self.inner.read)(self.inner, &mut buffer_size, buffer.as_mut_ptr()).into_with(|| buffer_size)
    }

    /// Write data to file
    ///
    /// Write `buffer` to file, increment the file pointer and return number of bytes written 
    ///
    /// # Arguments
    /// * `buffer`  Buffer to write to file
    ///
    /// # Errors
    /// * `uefi::Status::NoMedia`           The device has no media
    /// * `uefi::Status::DeviceError`       The device reported an error
    /// * `uefi::Status::VolumeCorrupted`   The filesystem structures are corrupted
    /// * `uefi::Status::WriteProtected`    Attempt to write to readonly file
    /// * `uefi::Status::AccessDenied`      The file was opened read only.
    /// * `uefi::Status::VolumeFull`        The volume is full
    pub fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        let mut buffer_size = buffer.len();
        (self.inner.write)(self.inner, &mut buffer_size, buffer.as_ptr()).into_with(|| buffer_size)
    }

    /// Get the file's current position
    ///
    /// # Errors
    /// * `uefi::Status::DeviceError`   An attempt was made to get the position of a deleted file
    pub fn get_position(&mut self) -> Result<u64> {
        let mut pos = 0u64;
        (self.inner.get_position)(self.inner, &mut pos).into_with(|| pos)
    }

    /// Sets the file's current position
    ///
    /// Set the position of this file handle to the absolute position specified by `position`.
    /// Seeking is not permitted outside the bounds of the file, except in the case
    /// of 0xFFFFFFFFFFFFFFFF, in which case the position is set to the end of the file
    ///
    /// # Arguments
    /// * `position` The new absolution position of the file handle
    ///
    /// # Errors
    /// * `uefi::Status::DeviceError`   An attempt was made to set the position of a deleted file
    pub fn set_position(&mut self, position: u64) -> Result<()> {
        (self.inner.set_position)(self.inner, position).into()
    }

    /// Flushes all modified data associated with the file handle to the device
    ///
    /// # Errors
    /// * `uefi::Status::NoMedia`           The device has no media
    /// * `uefi::Status::DeviceError`       The device reported an error
    /// * `uefi::Status::VolumeCorrupted`   The filesystem structures are corrupted
    /// * `uefi::Status::WriteProtected`    The file or medium is write protected
    /// * `uefi::Status::AccessDenied`      The file was opened read only
    /// * `uefi::Status::VolumeFull`        The volume is full
    pub fn flush(&mut self) -> Result<()> {
        (self.inner.flush)(self.inner).into()
    }
}

impl SimpleFileSystem {
    /// Open the root directory on a volume
    ///
    /// # Errors
    /// * `uefi::Status::Unsupported`   The volume does not support the requested filesystem type
    /// * `uefi::Status::NoMedia`       The device has no media
    /// * `uefi::Status::DeviceError`   The device reported an error
    /// * `uefi::Status::VolumeCorrupted`   The file system structures are corrupted
    /// * `uefi::Status::AccessDenied`  The service denied access to the file
    /// * `uefi::Status::OutOfResources`    The volume was not opened
    /// * `uefi::Status::MediaChanged`  The device has a different medium in it
    pub fn open_volume(&mut self) -> Result<File> {
        let mut ptr = 0usize;
        (self.open_volume)(self, &mut ptr).into_with(|| File { inner: unsafe { &mut *(ptr as *mut FileImpl)} })
    }
}

impl_proto! {
    protocol SimpleFileSystem {
        GUID = 0x0964e5b22,0x6459,0x11d2,[0x8e,0x39,0x00,0xa0,0xc9,0x69,0x72,0x3b];
    }
}
