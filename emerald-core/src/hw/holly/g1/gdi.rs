// .gdi format support

use std::{cmp::min, fs, path::Path};

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "src/hw/holly/g1/gdi.pest"]
pub struct GdiParser;

#[derive(Debug, Clone)]
pub struct GdiImage {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub offset: usize,
    pub is_audo_track: bool,
    pub control: usize,
    pub sector_size: usize,
    pub data: Vec<u8>,
    pub number: usize,
    pub adr: u8,
}

impl Track {
    pub fn header_size(&self) -> usize {
        return if self.sector_size == 2352 { 0x10 } else { 0 };
    }

    pub fn load_sectors(&self, lba: u32, count: u32, dest: &mut [u8]) -> u32 {
        let mut sector_start = ((lba - self.offset as u32) * self.sector_size as u32) as usize;
        let mut copied: usize = 0;

        match self.sector_size {
            2048 => {
                for _ in 0..count {
                    let chunk_size = min(self.sector_size as usize, dest.len() - copied as usize);
                    let dest_slice = &mut dest[copied as usize..(copied as usize + chunk_size)];
                    let src_slice =
                        &self.data[sector_start as usize..(sector_start as usize + chunk_size)];
                    dest_slice.copy_from_slice(src_slice);
                    copied += chunk_size;
                    sector_start += chunk_size;
                }
            }
            2352 => {
                for _ in 0..count {
                    let header = &self.data
                        [sector_start as usize..(sector_start as usize + self.header_size())];
                    assert!(header[0x0F] == 1 || header[0x0F] == 2);

                    let data_size = if header[0x0F] == 1 { 2048 } else { 2336 };
                    let chunk_size = min(data_size as usize, dest.len() - copied as usize);
                    let dest_slice = &mut dest[copied..(copied + chunk_size)];
                    let src_slice = &self.data[(sector_start + self.header_size())
                        ..(sector_start + self.header_size() + chunk_size)];
                    dest_slice.copy_from_slice(src_slice);
                    copied += chunk_size;
                    sector_start += self.sector_size;
                }
            }
            _ => panic!("Unimplemented"),
        }

        copied as u32
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AreaDescriptor {
    pub start_track: usize,
    pub end_track: usize,
    pub lead_in: usize,
    pub lead_out: usize,
}

impl GdiImage {
    pub fn get_corresponding_track(&self, lda: u32) -> &Track {
        assert!(self.tracks.len() > 0, "tracks list is empty");

        let mut idx: usize = 0;
        while idx + 1 < self.tracks.len() && self.tracks[idx + 1].offset <= lda as usize {
            idx += 1;
        }

        &self.tracks[usize::max(0, idx)]
    }

    pub fn load_sectors(&self, lba: u32, count: u32, dest: &mut [u8]) -> u32 {
        let track = self.get_corresponding_track(lba);
        return track.load_sectors(lba, count, dest);
    }
}

impl GdiParser {
    pub fn load_from_file(path: &str) -> GdiImage {
        let gdi_path = Path::new(path);
        let gdi_contents = fs::read_to_string(gdi_path).unwrap();
        let successful_parse = GdiParser::parse(Rule::gdi, &gdi_contents)
            .unwrap()
            .next()
            .unwrap();

        let mut tracks = vec![];
        let mut i = 1;

        for record in successful_parse.into_inner() {
            match record.as_rule() {
                Rule::track_line => {
                    let mut fields = record.into_inner();

                    let _ = fields.next().unwrap().as_str().parse::<usize>().unwrap();
                    let beginning_lba = fields.next().unwrap().as_str().parse::<usize>().unwrap();
                    let track_type = fields.next().unwrap().as_str().parse::<usize>().unwrap();
                    let sector_size = fields.next().unwrap().as_str().parse::<usize>().unwrap();
                    let pathpath = gdi_path.parent().unwrap().join(Path::new(
                        &fields.next().unwrap().as_str().trim_matches('"'),
                    ));

                    tracks.push(Track {
                        number: i,
                        offset: beginning_lba + 150,
                        is_audo_track: track_type == 0,
                        control: if track_type == 4 { 4 } else { 0 },
                        adr: 1,
                        sector_size,
                        data: fs::read(pathpath).unwrap(),
                    });
                    i += 1;
                }
                _ => {}
            }
        }

        //  panic!("");
        GdiImage { tracks: tracks }
    }
}
