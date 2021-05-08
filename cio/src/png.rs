use std::convert::TryInto;

#[derive(Debug, Clone)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub color_type: u8,
    pub interlace: bool,
    pub palette: Option<(Vec<u8>, u32)>,
    pub icc: Option<Vec<u8>>,
}

fn read_be_u32(input: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes(input[pos..pos + 4].try_into().expect("not enough byte"))
}

pub fn get_info(bytes: &[u8]) -> PngInfo {
    let width = read_be_u32(bytes, 16);
    let height = read_be_u32(bytes, 20);
    let depth = bytes[24];
    let color_type = bytes[25];
    let interlace = match bytes[28] {
        0 => false,
        1 => true,
        _ => false,
    };

    let mut palette: Option<(Vec<u8>, u32)> = None;
    let mut icc: Option<Vec<u8>> = None;
    let mut pos = 33;

    loop {
        let size = read_be_u32(bytes, pos);
        let name = std::str::from_utf8(&bytes[pos + 4..pos + 8]).unwrap();

        match name {
            "PLTE" => {
                palette = Some((bytes[pos + 8..pos + 8 + (size as usize)].into(), size / 3));
                pos += 8 + size as usize + 4;
            }
            "iCCP" => {
                let icc_start = bytes[pos + 8..pos + 8 + (size as usize)].into_iter().position(|&x| x == b'\x00');
                icc = icc_start.map(|start| bytes[pos + 8 + start + 1..pos + 8 + (size as usize)].into());
                pos += 8 + size as usize + 4;
            }
            "IDAT" => break,
            _ => pos += 8 + size as usize + 4,
        }
    }

    PngInfo {
        width,
        height,
        depth,
        color_type,
        interlace,
        palette,
        icc,
    }
}

pub fn get_idat(bytes: &[u8]) -> Vec<u8> {
    let mut pos = 33;
    let mut result = Vec::new();

    loop {
        let size = read_be_u32(bytes, pos);
        let name = std::str::from_utf8(&bytes[pos + 4..pos + 8]).unwrap();

        match name {
            "IDAT" => {
                result.push(&bytes[pos + 8..pos + 8 + (size as usize)]);
                pos += 8 + size as usize + 4;
            }
            "IEND" => break,
            _ => pos += 8 + size as usize + 4,
        }
    }

    result.concat()
}
