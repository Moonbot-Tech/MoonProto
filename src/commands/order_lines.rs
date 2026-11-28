use super::registry::CURRENT_PROTO_CMD_VER;
use super::strict_read::read_str8;
use super::trade::BaseCommandHeader;

#[derive(Debug, Clone)]
pub(crate) struct OrderLinesRequest {
    pub header: BaseCommandHeader,
    #[allow(dead_code)] // Parsed request shape; the SDK only sends it.
    pub market: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OrderLinesPage {
    pub header: BaseCommandHeader,
    pub data: Vec<u8>,
}

pub(crate) fn build_request(uid: u64, market: Option<&str>) -> Vec<u8> {
    let name = market.unwrap_or_default().as_bytes();
    let mut data = Vec::with_capacity(12 + name.len());
    data.push(57);
    data.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    data.extend_from_slice(&uid.to_le_bytes());
    data.push(name.len() as u8);
    data.extend_from_slice(name);
    data
}

pub(crate) fn read_request(input: &mut &[u8]) -> Option<OrderLinesRequest> {
    let header = BaseCommandHeader::read(input)?;
    let mut pos = 0;
    let market = read_str8(input, &mut pos)?;
    *input = &input[pos..];
    Some(OrderLinesRequest { header, market })
}

pub(crate) fn read_page(input: &mut &[u8]) -> Option<OrderLinesPage> {
    let header = BaseCommandHeader::read(input)?;
    if input.len() < 4 { return None; }
    let data = input.to_vec();
    *input = &[];
    Some(OrderLinesPage { header, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_encode_one_market_or_all_without_automatic_subscription() {
        for market in [None, Some("BTCUSDT")] {
            let wire = build_request(42, market);
            assert_eq!(wire[0], 57);
            let request = read_request(&mut wire.as_slice()).unwrap();
            assert_eq!(request.header.uid, 42);
            assert_eq!(request.market, market.unwrap_or_default());
            for len in 0..wire.len() {
                assert!(read_request(&mut &wire[..len]).is_none());
            }
        }
    }
}
