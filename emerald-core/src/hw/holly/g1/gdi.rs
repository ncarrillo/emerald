// .gdi format support

use std::{fs, path::Path};

use pest::Parser;
use pest_derive::Parser;

use super::gdrom::Gdrom;

#[derive(Parser)]
#[grammar = "src/hw/holly/g1/gdi.pest"]
pub struct GdiParser;

#[derive(Debug, Clone)]
pub struct GdiImage {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub fad_start: usize,
    pub is_audo_track: bool,
    pub control: usize,
    pub sector_size: usize,
    pub data: Vec<u8>,
    pub number: usize,
    pub adr: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct AreaDescriptor {
    pub start_track: usize,
    pub end_track: usize,
    pub lead_in: usize,
    pub lead_out: usize,
}

impl GdiImage {
    pub fn get_descriptor_for_area(&self, area: usize) -> AreaDescriptor {
        if area == 0 {
            // area 0 is normal density and doesn't have actual game data.
            // just the audio that plays when you pop the gdrom cd into a cdrom reader.
            AreaDescriptor {
                start_track: 1,
                end_track: 2,
                lead_in: 0x00,
                lead_out: 0x4650,
            }
        } else {
            AreaDescriptor {
                start_track: 3,
                end_track: self.tracks.len() - 1,
                lead_in: 0xb05e,
                lead_out: 0x861b4,
            }
        }
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
                        fad_start: Gdrom::lba_to_fad(beginning_lba), // fad is just lba + pregap,
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

        GdiImage { tracks: tracks }
    }
}
