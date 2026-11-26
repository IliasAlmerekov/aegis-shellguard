use crate::language::SourceLanguage;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    FilesystemDelete,
    FilesystemOverwrite,
    PermissionOrOwnershipChange,
    DeviceOrCriticalWrite,
    DatabaseDestructive,
    CodeExecution,
    CloudDestructive,
    ContainerDestructive,
    PackageDestructive,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OperationModifiers {
    pub recursive: bool,
    pub forced: bool,
    pub destructive_mode: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperandCertainty {
    Known,
    Partial,
    Dynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    pub line: u32,
    pub column: u32,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedTarget {
    pub language: SourceLanguage,
    pub source: String,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedOperation {
    pub kind: OperationKind,
    pub modifiers: OperationModifiers,
    pub certainty: OperandCertainty,
    pub span: ByteSpan,
    pub payload: Option<NestedTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterResult {
    pub operations: Vec<DetectedOperation>,
    pub parse_errors: u32,
    pub degradation: Option<AdapterDegradation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterDegradation {
    UnsupportedEncoding,
}

pub const KIND_FS_DELETE: u8 = 0;
pub const KIND_FS_OVERWRITE: u8 = 1;
pub const KIND_PERM_OWNER: u8 = 2;
pub const KIND_DEVICE_WRITE: u8 = 3;
pub const KIND_DB_DESTRUCTIVE: u8 = 4;
pub const KIND_CODE_EXEC: u8 = 5;
pub const KIND_CLOUD_DESTRUCTIVE: u8 = 6;
pub const KIND_CONTAINER_DESTRUCTIVE: u8 = 7;
pub const KIND_PACKAGE_DESTRUCTIVE: u8 = 8;

pub const CERT_KNOWN: u8 = 0;
pub const CERT_PARTIAL: u8 = 1;
pub const CERT_DYNAMIC: u8 = 2;

pub const MOD_RECURSIVE: u8 = 1 << 0;
pub const MOD_FORCED: u8 = 1 << 1;
pub const MOD_DESTRUCTIVE: u8 = 1 << 2;

#[must_use]
pub(crate) fn kind_to_wire(kind: OperationKind) -> u8 {
    match kind {
        OperationKind::FilesystemDelete => KIND_FS_DELETE,
        OperationKind::FilesystemOverwrite => KIND_FS_OVERWRITE,
        OperationKind::PermissionOrOwnershipChange => KIND_PERM_OWNER,
        OperationKind::DeviceOrCriticalWrite => KIND_DEVICE_WRITE,
        OperationKind::DatabaseDestructive => KIND_DB_DESTRUCTIVE,
        OperationKind::CodeExecution => KIND_CODE_EXEC,
        OperationKind::CloudDestructive => KIND_CLOUD_DESTRUCTIVE,
        OperationKind::ContainerDestructive => KIND_CONTAINER_DESTRUCTIVE,
        OperationKind::PackageDestructive => KIND_PACKAGE_DESTRUCTIVE,
    }
}

#[must_use]
pub(crate) fn kind_from_wire(code: u8) -> Option<OperationKind> {
    Some(match code {
        KIND_FS_DELETE => OperationKind::FilesystemDelete,
        KIND_FS_OVERWRITE => OperationKind::FilesystemOverwrite,
        KIND_PERM_OWNER => OperationKind::PermissionOrOwnershipChange,
        KIND_DEVICE_WRITE => OperationKind::DeviceOrCriticalWrite,
        KIND_DB_DESTRUCTIVE => OperationKind::DatabaseDestructive,
        KIND_CODE_EXEC => OperationKind::CodeExecution,
        KIND_CLOUD_DESTRUCTIVE => OperationKind::CloudDestructive,
        KIND_CONTAINER_DESTRUCTIVE => OperationKind::ContainerDestructive,
        KIND_PACKAGE_DESTRUCTIVE => OperationKind::PackageDestructive,
        _ => return None,
    })
}

#[must_use]
pub(crate) fn certainty_to_wire(certainty: OperandCertainty) -> u8 {
    match certainty {
        OperandCertainty::Known => CERT_KNOWN,
        OperandCertainty::Partial => CERT_PARTIAL,
        OperandCertainty::Dynamic => CERT_DYNAMIC,
    }
}

#[must_use]
pub(crate) fn certainty_from_wire(code: u8) -> Option<OperandCertainty> {
    Some(match code {
        CERT_KNOWN => OperandCertainty::Known,
        CERT_PARTIAL => OperandCertainty::Partial,
        CERT_DYNAMIC => OperandCertainty::Dynamic,
        _ => return None,
    })
}

#[must_use]
pub(crate) fn modifiers_to_wire(mods: OperationModifiers) -> u8 {
    let mut bits = 0u8;
    if mods.recursive {
        bits |= MOD_RECURSIVE;
    }
    if mods.forced {
        bits |= MOD_FORCED;
    }
    if mods.destructive_mode {
        bits |= MOD_DESTRUCTIVE;
    }
    bits
}

#[must_use]
pub(crate) fn modifiers_from_wire(bits: u8) -> OperationModifiers {
    OperationModifiers {
        recursive: bits & MOD_RECURSIVE != 0,
        forced: bits & MOD_FORCED != 0,
        destructive_mode: bits & MOD_DESTRUCTIVE != 0,
    }
}
