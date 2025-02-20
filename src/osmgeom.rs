//! Convert objects to [`geo`] format

use geo;
use geo::{Coord, LineString, MultiPolygon, Polygon, coord, polygon};
use std::error::Error;
use std::fs;
use std::str;

use crate::osm::{self, BoundingBox};

/// Generate a [`geo::Polygon`] from a .poly file
///
/// The .poly file must follow the format from
/// [Polygon_Filter_File_Format](https://wiki.openstreetmap.org/wiki/Osmosis/Polygon_Filter_File_Format).
pub fn read_multipolygon(filename: &str) -> Result<(String, MultiPolygon<i64>), Box<dyn Error>> {
    let src = fs::read_to_string(filename)?;
    let mut lines = src.lines();
    let name = String::from(lines.next().unwrap());

    let mut polygons: Vec<Polygon<i64>> = Vec::new();

    loop {
        let line = lines.next();
        if line.is_none() || line.unwrap().starts_with("END") {
            break;
        }
        let line = line.unwrap();
        let mut skip_polygon = false;
        if line.starts_with('!') {
            skip_polygon = true;
        }
        let polygon = read_polygon(&mut lines);
        if !skip_polygon {
            polygons.push(polygon);
        }
    }
    let multipolygon = MultiPolygon::new(polygons);

    Ok((name, multipolygon))
}

fn read_polygon(lines: &mut str::Lines) -> Polygon<i64> {
    let mut coords: Vec<Coord<i64>> = Vec::new();
    loop {
        let line = lines.next();
        if line.is_none() {
            panic!("Incomplete .poly file");
        }
        let line = line.unwrap();
        if line.starts_with("END") {
            break;
        }
        let mut c = line.split_whitespace();
        let x: f64 = c.next().unwrap().parse().unwrap();
        let y: f64 = c.next().unwrap().parse().unwrap();
        let x = osm::coord_to_decimicro(x);
        let y = osm::coord_to_decimicro(y);
        coords.push(coord!(x: i64::from(x), y: i64::from(y)));
    }
    let linestring = LineString::new(coords);
    Polygon::new(linestring, vec![])
}

/// Generate a [`geo::Polygon`] from a [`BoundingBox`]
pub fn bounding_box_to_polygon(bbox: &BoundingBox) -> Polygon<i64> {
    polygon![
        (x: i64::from(bbox.decimicro_minlon), y: i64::from(bbox.decimicro_minlat)),
        (x: i64::from(bbox.decimicro_minlon), y: i64::from(bbox.decimicro_maxlat)),
        (x: i64::from(bbox.decimicro_maxlon), y: i64::from(bbox.decimicro_maxlat)),
        (x: i64::from(bbox.decimicro_maxlon), y: i64::from(bbox.decimicro_minlat)),
        (x: i64::from(bbox.decimicro_minlon), y: i64::from(bbox.decimicro_minlat)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{CoordsIter, Intersects, point, polygon};

    #[test]
    fn read_africa() {
        let res = read_multipolygon("tests/resources/africa.poly").unwrap();
        assert_eq!("africa", res.0);
        assert_eq!(1, res.1.0.len()); // number of polygons

        let expected_polygon: Polygon<i64> = polygon![
        (x: 116009200, y: 339987500),
        (x: 116020700, y: 377781700),
        (x: 35259890, y: 377644400),
        (x: -19678260, y: 363217100),
        (x: -42878490, y: 362008200),
        (x: -56029400, y: 359877000),
        (x: -96186880, y: 359810200),
        (x: -155147330, y: 295008260),
        (x: -272620320, y: 308140000),
        (x: -232453600, y: -603167000),
        (x: 446394200, y: -570879800),
        (x: 667227660, y: -149037070),
        (x: 516302500, y: 125501500),
        (x: 442077500, y: 116786000),
        (x: 436541720, y: 125492040),
        (x: 433575410, y: 126349810),
        (x: 433383150, y: 127903770),
        (x: 431076020, y: 132105370),
        (x: 426791350, y: 135926020),
        (x: 425170840, y: 140886350),
        (x: 420446670, y: 147111450),
        (x: 398131190, y: 181622960),
        (x: 379028210, y: 222382700),
        (x: 347412610, y: 270315910),
        (x: 344757840, y: 280065270),
        (x: 347058090, y: 285760810),
        (x: 349374100, y: 294251900),
        (x: 348797030, y: 295570330),
        (x: 348858830, y: 296428570),
        (x: 348492400, y: 297866600),
        (x: 342428400, y: 312968150),
        (x: 327062930, y: 339752580),
        (x: 116009200, y: 339987500),
        ];
        let expected_multipolygon = MultiPolygon::new(vec![expected_polygon]);
        assert_eq!(expected_multipolygon, res.1);
    }
    #[test]
    fn read_canarias() {
        let res = read_multipolygon("tests/resources/canarias.poly").unwrap();
        assert_eq!("polygon", res.0);
        assert_eq!(9, res.1.0.len()); // number of polygons
        assert_eq!(8, res.1.0.get(0).unwrap().exterior().coords_count());
        assert_eq!(55, res.1.0.get(1).unwrap().exterior().coords_count());
        assert_eq!(9, res.1.0.get(2).unwrap().exterior().coords_count());
        assert_eq!(61, res.1.0.get(3).unwrap().exterior().coords_count());
        assert_eq!(69, res.1.0.get(4).unwrap().exterior().coords_count());
        assert_eq!(72, res.1.0.get(5).unwrap().exterior().coords_count());
        assert_eq!(24, res.1.0.get(6).unwrap().exterior().coords_count());
        assert_eq!(33, res.1.0.get(7).unwrap().exterior().coords_count());
        assert_eq!(29, res.1.0.get(8).unwrap().exterior().coords_count());
    }
    #[test]
    fn intersects_canarias() {
        let res = read_multipolygon("tests/resources/canarias.poly").unwrap();
        let polygon = res.1;
        assert_eq!(false, point!(x: 0 as i64, y: 0 as i64).intersects(&polygon));
        assert_eq!(
            true,
            point!(x: -166015000 as i64, y: 281876000 as i64).intersects(&polygon)
        );

        // bounding-box outside polygon

        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: -266015000,
            decimicro_maxlat: 0,
            decimicro_minlon: -366015000,
            decimicro_maxlon: -266015000,
        });
        assert_eq!(false, p.intersects(&polygon));

        // bounding-box consisting of a single point
        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 0,
            decimicro_maxlat: 0,
            decimicro_minlon: 0,
            decimicro_maxlon: 0,
        });
        assert_eq!(false, p.intersects(&polygon));

        // bounding-box consisting of a single line
        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 0,
            decimicro_maxlat: 0,
            decimicro_minlon: -166015200,
            decimicro_maxlon: -166015100,
        });
        assert_eq!(false, p.intersects(&polygon));

        // bounding-box inside polygon

        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 0,
            decimicro_maxlat: 281876000,
            decimicro_minlon: -166015000,
            decimicro_maxlon: 0,
        });
        assert_eq!(true, p.intersects(&polygon));

        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 281875000,
            decimicro_maxlat: 281876000,
            decimicro_minlon: -166015200,
            decimicro_maxlon: -166015100,
        });
        assert_eq!(true, p.intersects(&polygon));

        // bounding-box consisting of a single point
        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 281876000,
            decimicro_maxlat: 281876000,
            decimicro_minlon: -166015100,
            decimicro_maxlon: -166015100,
        });
        assert_eq!(true, p.intersects(&polygon));

        // bounding-box consisting of a single line
        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 281875000,
            decimicro_maxlat: 281876000,
            decimicro_minlon: -166015100,
            decimicro_maxlon: -166015100,
        });
        assert_eq!(true, p.intersects(&polygon));

        // bounding-box consisting of a single line
        let p = bounding_box_to_polygon(&BoundingBox {
            decimicro_minlat: 281876000,
            decimicro_maxlat: 281876000,
            decimicro_minlon: -166015200,
            decimicro_maxlon: -166015100,
        });
        assert_eq!(true, p.intersects(&polygon));
    }
}
