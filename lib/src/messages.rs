use std::array::TryFromSliceError;

use jiff::Timestamp;

/// Error from parsing request or response data.
#[derive(Debug, Clone, thiserror::Error)]
#[error("data parsing error")]
pub enum ParseError {
    /// Jiff timestamp validity errors.
    #[error("invalid timestamp: {0}")]
    Timestamp(#[from] jiff::Error),

    /// Not enough data in the packet.
    #[error("too little data: {0}")]
    NeedData(#[from] TryFromSliceError),
}

/// A timesimp request.
///
/// Serializes to the timestamp in microseconds, as a 64-bit signed integer, in big endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    /// The client timestamp.
    pub client: Timestamp,
}

impl Request {
    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; 8] {
        self.client.as_microsecond().to_be_bytes()
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: [u8; 8]) -> Result<Self, ParseError> {
        Ok(Self {
            client: Timestamp::from_microsecond(i64::from_be_bytes(bytes))?,
        })
    }
}

impl From<Request> for Vec<u8> {
    fn from(request: Request) -> Self {
        request.to_bytes().to_vec()
    }
}

impl TryFrom<&[u8]> for Request {
    type Error = ParseError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::from_bytes(bytes[..8].try_into()?)
    }
}

/// A timesimp response.
///
/// Serializes to the two timestamps, in microseconds, as 64-bit signed integers, in big endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Response {
    /// The client timestamp, identical to that in the request.
    pub client: Timestamp,

    /// The server timestamp.
    pub server: Timestamp,
}

impl Response {
    /// Serialize to bytes.
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.client.as_microsecond().to_be_bytes());
        bytes[8..].copy_from_slice(&self.server.as_microsecond().to_be_bytes());
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, ParseError> {
        Ok(Self {
            client: Timestamp::from_microsecond(i64::from_be_bytes(
                bytes[..8].try_into().unwrap(),
            ))?,
            server: Timestamp::from_microsecond(i64::from_be_bytes(
                bytes[8..].try_into().unwrap(),
            ))?,
        })
    }
}

impl From<Response> for Vec<u8> {
    fn from(response: Response) -> Self {
        response.to_bytes().to_vec()
    }
}

impl TryFrom<&[u8]> for Response {
    type Error = ParseError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::from_bytes(bytes[..16].try_into()?)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::*;

    fn microround(ts: Timestamp) -> Timestamp {
        let micros = ts.as_microsecond();
        Timestamp::from_microsecond(micros).unwrap()
    }

    #[test]
    fn round_trip_request() {
        let request = Request {
            client: microround(Timestamp::now()),
        };
        let bytes = request.to_bytes();
        assert_eq!(request, Request::try_from(&bytes[..]).unwrap());
    }

    #[test]
    fn round_trip_response() {
        let response = Response {
            client: microround(Timestamp::now()),
            server: microround(Timestamp::now()),
        };
        let bytes = response.to_bytes();
        assert_eq!(response, Response::try_from(&bytes[..]).unwrap());
    }

    #[test]
    fn specific_requests() {
        let request = Request {
            client: microround(Timestamp::from_str("2025-04-28T03:11:00.564184Z").unwrap()),
        };
        let bytes = vec![0, 6, 51, 206, 8, 149, 148, 216];
        assert_eq!(request, Request::try_from(&bytes[..]).unwrap());
    }
}
