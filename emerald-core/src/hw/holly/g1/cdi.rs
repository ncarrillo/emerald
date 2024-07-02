use std::assert_matches::assert_matches;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::{fs::File, io::SeekFrom};

#[derive(Debug, Clone)]
pub struct CdiImage {
    pub tracks: Vec<Track>,
}

#[derive(Copy, Clone, Debug)]
pub struct Track {}

pub struct CdiParser;

impl CdiParser {
    const CDI_VERSION2: u32 = 0x80000004;
    const CDI_VERSION3: u32 = 0x80000005;
    const CDI_VERSION35: u32 = 0x80000006;

    pub fn load_from_file(path: &str) -> CdiImage {
        let cdi_path = Path::new(path);
        let mut cdi_file = File::open(cdi_path).unwrap();

        cdi_file.seek(SeekFrom::End(-8)).unwrap();

        let mut buffer = [0u8; 4];
        let bytes_read = cdi_file.read(&mut buffer).unwrap();
        assert!(bytes_read == 4);

        let version = u32::from_le_bytes(buffer);

        assert_matches!(
            version,
            Self::CDI_VERSION2 | Self::CDI_VERSION3 | Self::CDI_VERSION35
        );

        let mut buffer = [0u8; 4];
        let bytes_read = cdi_file.read(&mut buffer).unwrap();
        assert!(bytes_read == 4);

        let header_offset = u32::from_le_bytes(buffer);

        println!("{:08x} {:08x}", version, header_offset);

        unimplemented!()
    }
}
