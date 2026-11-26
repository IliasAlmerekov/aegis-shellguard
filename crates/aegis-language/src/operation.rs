use crate::language::SourceLanguage;
use crate::protocol::{DecodeError, EncodeError};

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
pub fn kind_to_wire(kind: OperationKind) -> u8 {
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
pub fn kind_from_wire(code: u8) -> Option<OperationKind> {
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
pub fn certainty_to_wire(certainty: OperandCertainty) -> u8 {
    match certainty {
        OperandCertainty::Known => CERT_KNOWN,
        OperandCertainty::Partial => CERT_PARTIAL,
        OperandCertainty::Dynamic => CERT_DYNAMIC,
    }
}

#[must_use]
pub fn certainty_from_wire(code: u8) -> Option<OperandCertainty> {
    Some(match code {
        CERT_KNOWN => OperandCertainty::Known,
        CERT_PARTIAL => OperandCertainty::Partial,
        CERT_DYNAMIC => OperandCertainty::Dynamic,
        _ => return None,
    })
}

#[must_use]
pub fn modifiers_to_wire(mods: OperationModifiers) -> u8 {
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
pub fn modifiers_from_wire(bits: u8) -> OperationModifiers {
    OperationModifiers {
        recursive: bits & MOD_RECURSIVE != 0,
        forced: bits & MOD_FORCED != 0,
        destructive_mode: bits & MOD_DESTRUCTIVE != 0,
    }
}

pub fn encode_adapter_result(result: &AdapterResult) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    out.extend_from_slice(&result.parse_errors.to_le_bytes());
    out.push(match result.degradation {
        None => 0,
        Some(AdapterDegradation::UnsupportedEncoding) => 1,
    });
    let op_count = u32::try_from(result.operations.len())
        .map_err(|_| EncodeError::FieldOverflow { field: "op_count" })?;
    out.extend_from_slice(&op_count.to_le_bytes());
    for op in &result.operations {
        encode_operation(&mut out, op)?;
    }
    Ok(out)
}

fn encode_operation(out: &mut Vec<u8>, op: &DetectedOperation) -> Result<(), EncodeError> {
    out.push(kind_to_wire(op.kind));
    out.push(modifiers_to_wire(op.modifiers));
    out.push(certainty_to_wire(op.certainty));
    encode_span(out, &op.span)?;
    match &op.payload {
        Some(target) => {
            out.push(1);
            out.push(crate::protocol::language_to_wire(target.language));
            let len =
                u32::try_from(target.source.len()).map_err(|_| EncodeError::FieldOverflow {
                    field: "payload.source.len",
                })?;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(target.source.as_bytes());
            encode_span(out, &target.span)?;
        }
        None => out.push(0),
    }
    Ok(())
}

fn encode_span(out: &mut Vec<u8>, span: &ByteSpan) -> Result<(), EncodeError> {
    out.extend_from_slice(&span.line.to_le_bytes());
    out.extend_from_slice(&span.column.to_le_bytes());
    let start = u32::try_from(span.byte_start).map_err(|_| EncodeError::FieldOverflow {
        field: "byte_start",
    })?;
    out.extend_from_slice(&start.to_le_bytes());
    let end = u32::try_from(span.byte_end)
        .map_err(|_| EncodeError::FieldOverflow { field: "byte_end" })?;
    out.extend_from_slice(&end.to_le_bytes());
    Ok(())
}

pub fn decode_adapter_result(buf: &[u8]) -> Result<AdapterResult, DecodeError> {
    let mut cur = Cursor::new(buf);
    let parse_errors = cur.read_u32("parse_errors")?;
    let degradation = match cur.read_u8("degradation")? {
        0 => None,
        1 => Some(AdapterDegradation::UnsupportedEncoding),
        _ => return Err(DecodeError::InvalidPayload("unknown adapter degradation")),
    };
    let op_count = cur.read_u32("op_count")? as usize;
    let mut operations = Vec::with_capacity(op_count.min(512));
    for _ in 0..op_count {
        operations.push(decode_operation(&mut cur)?);
    }
    if !cur.is_empty() {
        return Err(DecodeError::InvalidPayload(
            "trailing bytes after AdapterResult",
        ));
    }
    Ok(AdapterResult {
        operations,
        parse_errors,
        degradation,
    })
}

fn decode_operation(cur: &mut Cursor<'_>) -> Result<DetectedOperation, DecodeError> {
    let kind = kind_from_wire(cur.read_u8("kind")?).ok_or(DecodeError::InvalidPayload(
        "unknown operation kind discriminant",
    ))?;
    let modifiers = modifiers_from_wire(cur.read_u8("modifiers")?);
    let certainty = certainty_from_wire(cur.read_u8("certainty")?).ok_or(
        DecodeError::InvalidPayload("unknown certainty discriminant"),
    )?;
    let span = decode_span(cur)?;
    let has_payload = cur.read_u8("has_payload")?;
    let payload = match has_payload {
        0 => None,
        1 => Some(decode_nested_target(cur)?),
        _ => return Err(DecodeError::InvalidPayload("has_payload must be 0 or 1")),
    };
    Ok(DetectedOperation {
        kind,
        modifiers,
        certainty,
        span,
        payload,
    })
}

fn decode_nested_target(cur: &mut Cursor<'_>) -> Result<NestedTarget, DecodeError> {
    let language = crate::protocol::wire_to_language(cur.read_u8("payload.language")?)
        .ok_or(DecodeError::InvalidPayload("unknown payload language tag"))?;
    let len = cur.read_u32("payload.source.len")? as usize;
    let source_bytes = cur.read_bytes(len, "payload.source")?;
    let source = String::from_utf8(source_bytes.to_vec())
        .map_err(|_| DecodeError::InvalidPayload("payload source is not valid UTF-8"))?;
    let span = decode_span(cur)?;
    Ok(NestedTarget {
        language,
        source,
        span,
    })
}

fn decode_span(cur: &mut Cursor<'_>) -> Result<ByteSpan, DecodeError> {
    let line = cur.read_u32("span.line")?;
    let column = cur.read_u32("span.column")?;
    let byte_start = cur.read_u32("span.byte_start")? as usize;
    let byte_end = cur.read_u32("span.byte_end")? as usize;
    Ok(ByteSpan {
        line,
        column,
        byte_start,
        byte_end,
    })
}

struct Cursor<'a> {
    buf: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    fn read_u8(&mut self, field: &'static str) -> Result<u8, DecodeError> {
        let (head, rest) = self
            .buf
            .split_first()
            .ok_or(DecodeError::InvalidPayload(field))?;
        *self = Self::new(rest);
        Ok(*head)
    }

    fn read_u32(&mut self, field: &'static str) -> Result<u32, DecodeError> {
        let Some((&bytes, rest)) = self.buf.split_first_chunk::<4>() else {
            return Err(DecodeError::InvalidPayload(field));
        };
        let value = u32::from_le_bytes(bytes);
        *self = Self::new(rest);
        Ok(value)
    }

    fn read_bytes(&mut self, len: usize, field: &'static str) -> Result<&'a [u8], DecodeError> {
        if self.buf.len() < len {
            return Err(DecodeError::InvalidPayload(field));
        }
        let (head, rest) = self.buf.split_at(len);
        *self = Self::new(rest);
        Ok(head)
    }
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
