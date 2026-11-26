use crate::language::SourceLanguage;

pub const MAGIC: [u8; 4] = *b"AELW";

pub const PROTOCOL_VERSION: u16 = 2;

pub const HEADER_LEN: usize = 15;

pub const MAX_SOURCE_BYTES: usize = 1 << 20;

pub const MAX_FRAME_PAYLOAD: usize = MAX_SOURCE_BYTES + 1;

const _: () = assert!(MAX_FRAME_PAYLOAD <= u32::MAX as usize);

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Parse {
        language: SourceLanguage,
        source: Vec<u8>,
    },
    Analyze {
        language: SourceLanguage,
        source: Vec<u8>,
    },
}

impl Request {
    pub const KIND_PARSE: u8 = 0x01;
    pub const KIND_ANALYZE: u8 = 0x02;
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Parsed {
        error_count: u32,
    },
    ParseFailed,
    Analyzed {
        result: crate::operation::AdapterResult,
    },
    UnsupportedLanguage,
}

impl Response {
    pub const KIND_PARSED: u8 = 0x81;
    pub const KIND_PARSE_FAILED: u8 = 0x82;
    pub const KIND_ANALYZED: u8 = 0x83;
    pub const KIND_UNSUPPORTED: u8 = 0x84;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame<T> {
    pub request_id: u32,
    pub message: T,
    pub consumed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("bad frame magic; expected {:?}, got {got:?}", MAGIC)]
    BadMagic { got: [u8; 4] },
    #[error("unsupported wire-format version {version} (supported {supported})", supported = PROTOCOL_VERSION)]
    UnsupportedVersion { version: u16 },
    #[error("oversized frame payload: declared {declared} bytes, max {max}")]
    Oversized { declared: u32, max: u32 },
    #[error("invalid message kind tag {tag:#04x}")]
    InvalidKind { tag: u8 },
    #[error("invalid payload: {0}")]
    InvalidPayload(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("frame payload ({actual} bytes) exceeds the {max}-byte ceiling")]
    Oversized { actual: usize, max: usize },
    #[error("wire field {field} overflows u32")]
    FieldOverflow { field: &'static str },
}

pub fn encode_request(request_id: u32, request: &Request) -> Result<Vec<u8>, EncodeError> {
    let (kind, language, source) = match request {
        Request::Parse { language, source } => (Request::KIND_PARSE, *language, source.as_slice()),
        Request::Analyze { language, source } => {
            (Request::KIND_ANALYZE, *language, source.as_slice())
        }
    };
    let mut payload = Vec::with_capacity(1 + source.len());
    payload.push(language_to_wire(language));
    payload.extend_from_slice(source);
    encode_frame(request_id, kind, &payload)
}

pub fn decode_request(buf: &[u8]) -> Result<Option<DecodedFrame<Request>>, DecodeError> {
    let (request_id, kind, payload, consumed) = match decode_header(buf)? {
        Some(h) => h,
        None => return Ok(None),
    };

    let message = match kind {
        Request::KIND_PARSE => {
            let (language, source) = decode_lang_source_payload(payload)?;
            Request::Parse { language, source }
        }
        Request::KIND_ANALYZE => {
            let (language, source) = decode_lang_source_payload(payload)?;
            Request::Analyze { language, source }
        }
        other => return Err(DecodeError::InvalidKind { tag: other }),
    };

    Ok(Some(DecodedFrame {
        request_id,
        message,
        consumed,
    }))
}

pub fn encode_response(request_id: u32, response: &Response) -> Result<Vec<u8>, EncodeError> {
    let (kind, payload): (u8, Vec<u8>) = match response {
        Response::Parsed { error_count } => {
            (Response::KIND_PARSED, error_count.to_le_bytes().to_vec())
        }
        Response::ParseFailed => (Response::KIND_PARSE_FAILED, Vec::new()),
        Response::Analyzed { result } => (
            Response::KIND_ANALYZED,
            crate::operation::encode_adapter_result(result)?,
        ),
        Response::UnsupportedLanguage => (Response::KIND_UNSUPPORTED, Vec::new()),
    };
    encode_frame(request_id, kind, &payload)
}

pub fn decode_response(buf: &[u8]) -> Result<Option<DecodedFrame<Response>>, DecodeError> {
    let (request_id, kind, payload, consumed) = match decode_header(buf)? {
        Some(h) => h,
        None => return Ok(None),
    };

    let message = match kind {
        Response::KIND_PARSED => {
            let error_count = u32::from_le_bytes(payload.try_into().map_err(|_| {
                DecodeError::InvalidPayload("Parsed response payload must be 4 bytes (error_count)")
            })?);
            Response::Parsed { error_count }
        }
        Response::KIND_PARSE_FAILED => {
            if !payload.is_empty() {
                return Err(DecodeError::InvalidPayload(
                    "ParseFailed response payload must be empty",
                ));
            }
            Response::ParseFailed
        }
        Response::KIND_ANALYZED => {
            let result = crate::operation::decode_adapter_result(payload)?;
            Response::Analyzed { result }
        }
        Response::KIND_UNSUPPORTED => {
            if !payload.is_empty() {
                return Err(DecodeError::InvalidPayload(
                    "UnsupportedLanguage response payload must be empty",
                ));
            }
            Response::UnsupportedLanguage
        }
        other => return Err(DecodeError::InvalidKind { tag: other }),
    };

    Ok(Some(DecodedFrame {
        request_id,
        message,
        consumed,
    }))
}

fn encode_frame(request_id: u32, kind: u8, payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(EncodeError::Oversized {
            actual: payload.len(),
            max: MAX_FRAME_PAYLOAD,
        });
    }
    let payload_len = payload.len() as u32;
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

type DecodedHeader<'a> = (u32, u8, &'a [u8], usize);

fn decode_header(buf: &[u8]) -> Result<Option<DecodedHeader<'_>>, DecodeError> {
    let Some(&[m0, m1, m2, m3, v0, v1, r0, r1, r2, r3, kind, l0, l1, l2, l3]) =
        buf.first_chunk::<HEADER_LEN>()
    else {
        return Ok(None);
    };
    let magic = [m0, m1, m2, m3];
    if magic != MAGIC {
        return Err(DecodeError::BadMagic { got: magic });
    }
    let version = u16::from_le_bytes([v0, v1]);
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion { version });
    }
    let request_id = u32::from_le_bytes([r0, r1, r2, r3]);
    let payload_len = u32::from_le_bytes([l0, l1, l2, l3]);
    if payload_len as usize > MAX_FRAME_PAYLOAD {
        return Err(DecodeError::Oversized {
            declared: payload_len,
            max: MAX_FRAME_PAYLOAD as u32,
        });
    }
    let total = HEADER_LEN + payload_len as usize;
    let Some(payload) = buf.get(HEADER_LEN..total) else {
        return Ok(None);
    };
    Ok(Some((request_id, kind, payload, total)))
}

fn decode_lang_source_payload(payload: &[u8]) -> Result<(SourceLanguage, Vec<u8>), DecodeError> {
    let (lang_byte, source) = payload.split_first().ok_or(DecodeError::InvalidPayload(
        "request payload missing language tag",
    ))?;
    let language = wire_to_language(*lang_byte).ok_or(DecodeError::InvalidPayload(
        "unknown language tag in request",
    ))?;
    Ok((language, source.to_vec()))
}

pub(crate) fn language_to_wire(language: SourceLanguage) -> u8 {
    match language {
        SourceLanguage::Python => 0x00,
        SourceLanguage::JavaScript => 0x01,
        SourceLanguage::TypeScript => 0x02,
        SourceLanguage::Bash => 0x03,
    }
}

pub(crate) fn wire_to_language(tag: u8) -> Option<SourceLanguage> {
    match tag {
        0x00 => Some(SourceLanguage::Python),
        0x01 => Some(SourceLanguage::JavaScript),
        0x02 => Some(SourceLanguage::TypeScript),
        0x03 => Some(SourceLanguage::Bash),
        _ => None,
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
