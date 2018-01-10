//! Performs operational transform.

use std::collections::HashMap;
use std::borrow::ToOwned;
use std::cmp;

use super::doc::*;
use super::stepper::*;
use super::compose;
use super::normalize;
use super::schema::*;
use super::writer::*;

use term_painter::ToStyle;
use term_painter::Color::*;
use term_painter::Attr::*;
use std::collections::HashSet;



fn parse_classes(input: &str) -> HashSet<String> {
    input
        .split_whitespace()
        .filter(|x| !x.is_empty())
        .map(|x| x.to_owned())
        .collect()
}

fn format_classes(set: &HashSet<String>) -> String {
    let mut classes: Vec<String> = set.iter().cloned().collect();
    classes.sort();
    classes.join(" ")
}


#[derive(Clone, Debug)]
struct Track {
    tag_a: Option<Tag>,
    tag_real: Option<Tag>,
    tag_b: Option<Tag>,
    is_original_a: bool,
    is_original_b: bool,
}

impl Track {
    // TODO dumb, remove this
    fn _anything(&self) -> Option<Tag> {
        self.tag_a.clone().or(self.tag_real.clone()).or(self.tag_b.clone())
    }
}

struct Transform {
    tracks: Vec<Track>,
    a_del: DelWriter,
    a_add: AddWriter,
    b_del: DelWriter,
    b_add: AddWriter,
}

// TODO move to schema
fn get_real_merge(a: &Tag, b: &Tag) -> Option<Tag> {
    if a.0.get("tag") == b.0.get("tag") && a.0.get("tag").map(|x| x == "span").unwrap_or(false) {
        let c_a: String = a.0.get("class").unwrap_or(&"".to_string()).clone();
        let c_b: String = b.0.get("class").unwrap_or(&"".to_string()).clone();

        let mut c = parse_classes(&c_a);
        c.extend(parse_classes(&c_b));
        Some(Tag(hashmap! {
            "tag".to_string() => "span".to_string(),
            "class".to_string() => format_classes(&c),
        }))
    } else {
        None
    }
}

impl Transform {
    fn new() -> Transform {
        Transform {
            tracks: vec![],
            a_del: DelWriter::new(),
            a_add: AddWriter::new(),
            b_del: DelWriter::new(),
            b_add: AddWriter::new(),
        }
    }

    fn enter(&mut self, name: &Tag) {
        let last = self.tracks
            .iter()
            .rposition(|x| x.tag_real.is_some())
            .and_then(|x| Some(x + 1))
            .unwrap_or(0);

        //   iterA.apply(insrA);
        //   iterA.apply(insrB);
        //   delrA.enter();
        //   delrB.enter();
        self.tracks.insert(
            last,
            Track {
                tag_a: Some(name.clone()),
                tag_real: Some(name.clone()),
                tag_b: Some(name.clone()),
                is_original_a: true,
                is_original_b: true,
            },
        );

        self.a_del.begin();
        self.a_add.begin();
        self.b_del.begin();
        self.b_add.begin();
    }

    // TODO maybe take "real" value as input?
    fn enter_a(&mut self, a: &Tag, b: Option<Tag>) {
        let last = self.tracks
            .iter()
            .rposition(|x| x.tag_real.is_some())
            .and_then(|x| Some(x + 1))
            .unwrap_or(0);

        // TODO merge this functionality elsewhere
        let real = if let Some(ref b) = b {
            Some(get_real_merge(a, b).unwrap_or_else(|| a.clone()))
        } else {
            Some(a.clone())
        };

        if let Some(last) = self.tracks.last() {
            if a.tag_type() == last._anything().unwrap().tag_type() {
                println!("-----> UGH {:?}", last);
                panic!("Should not have consecutive similar tracks.");
            }
        }

        self.tracks.insert(
            last,
            Track {
                tag_a: Some(a.clone()),
                tag_real: real,
                tag_b: b.clone(),
                is_original_a: true,
                is_original_b: false,
            },
        );

        self.a_del.begin();
        self.a_add.begin();
        if b.is_some() {
            self.b_del.begin();
        }
        self.b_add.begin();
    }

    fn enter_b(&mut self, a: Option<Tag>, b: Tag) {
        println!("ENTER B");

        let last = self.tracks
            .iter()
            .rposition(|x| x.tag_real.is_some())
            .and_then(|x| Some(x + 1))
            .unwrap_or(0);

        // TODO merge this functionality elsewhere
        let real = if let Some(ref a) = a {
            Some(get_real_merge(a, &b).unwrap_or_else(|| b.clone()))
        } else {
            Some(b.clone())
        };

        println!("dump all {:?}", self.tracks);

        if let Some(last) = self.tracks.last() {
            if b.tag_type() == last.tag_a.as_ref().or(last.tag_real.as_ref()).or(last.tag_b.as_ref()).unwrap().tag_type() {
                println!("-----> UGH {:?}", last);
                panic!("Should not have consecutive similar tracks.");
            }
        }

        self.tracks.insert(
            last,
            Track {
                tag_a: a.clone(),
                tag_real: real,
                tag_b: Some(b.clone()),
                is_original_a: false,
                is_original_b: true,
            },
        );

        if a.is_some() {
            self.a_del.begin();
        }
        self.a_add.begin();
        self.b_del.begin();
        self.b_add.begin();
    }

    // Close the topmost track.
    fn abort(&mut self) -> (Option<Tag>, Option<Tag>, Option<Tag>) {
        let track = self.tracks.pop().unwrap();

        println!("aborting: {:?}", track);
        if let Some(ref real) = track.tag_real {
            // if track.tag_a.is_some() {
            //     self.a_del.close();
            // }
            self.a_add.close(real.to_attrs()); // fake

            // if track.tag_b.is_some() {
            //     self.b_del.close();
            // }
            self.b_add.close(real.to_attrs()); // fake

            // } else {
            //     self.a_add.close(map! { "tag" => track.tag_a}); // fake
            // }
            // if (a) {
            //   insrA.alter(r, {}).close();
            // } else {
            //   insrA.close();
            // }
            // if (b) {
            //   insrB.alter(r, {}).close();
            // } else {
            //   insrB.close();
            // }
        }
        (track.tag_a, track.tag_real, track.tag_b)
    }

    fn unenter_a(&mut self) {
        self.a_del.begin();
        let track = self.tracks.last_mut().unwrap();
        track.tag_a = track.tag_real.clone();
    }

    fn unenter_b(&mut self, ty: TrackType) {
        self.b_del.begin();
        let track = self.next_track_b_by_type(ty).unwrap();
        track.tag_b = track.tag_real.clone();
    }

    fn skip_a(&mut self, n: usize) {
        self.a_del.skip(n);
        self.a_add.skip(n);
    }

    fn skip_b(&mut self, n: usize) {
        self.b_del.skip(n);
        self.b_add.skip(n);
    }

    fn with_group_a(&mut self, span: &AddSpan) {
        self.a_add.with_group(span);
    }

    fn with_group_b(&mut self, span: &AddSpan) {
        self.b_add.with_group(span);
    }

    fn group_a(&mut self, attrs: &Attrs, span: &AddSpan) {
        self.a_add.group(attrs, span);
    }

    fn group_b(&mut self, attrs: &Attrs, span: &AddSpan) {
        self.b_add.group(attrs, span);
    }

    fn chars_a(&mut self, chars: &str) {
        self.a_add.chars(chars);
    }

    fn chars_b(&mut self, chars: &str) {
        self.b_add.chars(chars);
    }

    fn current(&self) -> Option<Track> {
        let value = self.tracks.last();
        if let Some(track) = value {
            Some((*track).clone())
        } else {
            None
        }
    }

    fn close(&mut self) {
        let (track, index) = self.top_track_a();

        if track.is_original_a && track.tag_real == track.tag_a {
            println!("hey {:?} {:?}", track.tag_real, track.tag_a);
            self.a_del.exit();
            self.a_add.exit();
        } else {
            println!("LOVE");
            self.a_del.close();
            self.a_add.close(track.tag_real.clone().unwrap().to_attrs());
        }

        if track.is_original_b && track.tag_real == track.tag_b {
            println!("hey");
            self.b_del.exit();
            self.b_add.exit();
        } else {
            println!("NO");
            self.b_del.close();
            self.b_add.close(track.tag_real.clone().unwrap().to_attrs());
        }

        self.tracks.remove(index);
    }

    fn top_track_real(&self) -> Option<(Track, usize)> {
        self.tracks.iter()
            .rposition(|x| x.tag_real.is_some())
            .map(|index| (self.tracks[index].clone(), index))
    }

    fn top_track_a(&mut self) -> (Track, usize) {
        let index = self.tracks.iter().rposition(|x| x.tag_a.is_some()).unwrap();
        (self.tracks[index].clone(), index)
    }

    fn next_track_a(&mut self) -> Option<&mut Track> {
        if let Some(index) = self.tracks.iter().position(|x| x.tag_a.is_none()) {
            Some(&mut self.tracks[index])
        } else {
            None
        }
    }

    fn next_track_a_type(&mut self) -> Option<TrackType> {
        if let Some(track) = self.next_track_a() {
            track.tag_real.as_ref().and_then(|x| x.tag_type())
        } else {
            None
        }
    }

    fn top_track_b(&mut self) -> (Track, usize) {
        let index = self.tracks.iter().rposition(|x| x.tag_b.is_some()).unwrap();
        (self.tracks[index].clone(), index)
    }

    fn next_track_b(&mut self) -> Option<&mut Track> {
        if let Some(index) = self.tracks.iter().position(|x| x.tag_b.is_none()) {
            Some(&mut self.tracks[index])
        } else {
            None
        }
    }

    fn next_track_b_type(&mut self) -> Option<TrackType> {
        if let Some(track) = self.next_track_b() {
            track.tag_real.as_ref().and_then(|x| x.tag_type())
        } else {
            None
        }
    }

    // TODO does this replace its counterpart?
    fn next_track_a_by_type(&mut self, arg: TrackType) -> Option<&mut Track> {
        if let Some(track) = self.tracks.iter()
            .position(|x| x.tag_a.is_none() && x.tag_real.as_ref().and_then(|x| x.tag_type()) == Some(arg)) {
            Some(&mut self.tracks[track])
        } else {
            None
        }
    }

    // TODO does this replace its counterpart?
    fn next_track_b_by_type(&mut self, arg: TrackType) -> Option<&mut Track> {
        if let Some(track) = self.tracks.iter()
            .position(|x| x.tag_b.is_none() && x.tag_real.as_ref().and_then(|x| x.tag_type()) == Some(arg)) {
            Some(&mut self.tracks[track])
        } else {
            None
        }
    }

    fn close_a(&mut self) {
        println!("TRACKS CLOSE A: {:?}", self.tracks);
        let (track, index) = self.top_track_a();

        // Determine whether to split tags for this track type.
        // TODO do the same for track opening?
        // TODO remove do_split checks from this class? no-schema knowledge possible?
        let track_split = if let Some(tag) = track.tag_real.clone() {
            tag.tag_type().map_or(false, |x| x.do_split())
        } else {
            true
        };

        if track.is_original_a && (track_split || track.tag_b.is_none()) {
            // && track.tag_real == track.tag_a {
            self.a_del.exit();
            self.a_add.exit();
        } else {
            self.a_del.close();
            if track_split || track.tag_b.is_none() {
                self.a_add.close(track.tag_real.clone().unwrap().to_attrs());
            }
        }

        // if track.is_original_b {
        //     self.b_del.close();
        // }
        println!("CLOSES THE B {:?}", self.b_add);
        if track_split || track.tag_b.is_none() {
            self.b_add.close(track.tag_real.clone().unwrap().to_attrs());
        }

        if track.tag_b.is_none() {
            self.tracks.remove(index);
        } else {
            self.tracks[index].is_original_a = false;
            if track_split {
                self.tracks[index].is_original_b = false;
            }
            self.tracks[index].tag_a = None;
            if track_split {
                self.tracks[index].tag_real = None;
            }
        }
    }

    fn close_b(&mut self) {
        println!("close_b:");
        for t in &self.tracks {
            println!(" - {:?}", t);
        }

        let (track, index) = self.top_track_b();

        println!("CLOSES THE B {:?}", self.b_del);
        println!("CLOSES THE B {:?}", self.b_add);

        // Determine whether to split tags for this track type.
        // TODO do the same for track opening?
        // NOTE i might have done this already
        // TODO remove do_split checks from this class? no-schema knowledge possible?
        let track_split = if let Some(tag) = track.tag_real.clone() {
            tag.tag_type().map_or(false, |x| x.do_split())
        } else {
            true
        };
        println!("TAG {:?}", track);
        println!(
            "tag {:?}",
            track
                .tag_real
                .clone()
                .unwrap()
                .tag_type()
                .unwrap()
                .do_split()
        );

        if track.is_original_b && (track_split || track.tag_a.is_none()) {
            // && track.tag_real == track.tag_b {
            self.b_del.exit();
            self.b_add.exit();
        } else {
            println!("1");

            self.b_del.close();
            if track_split || track.tag_a.is_none() {
                self.b_add.close(track.tag_real.clone().unwrap().to_attrs());
            }
            println!("2 {:?}", self.b_del);
        }

        // if track.is_original_a {
        //     self.a_del.close();
        // }
        if track_split || track.tag_a.is_none() {
            self.a_add.close(track.tag_real.clone().unwrap().to_attrs());
        }

        if track.tag_a.is_none() {
            self.tracks.remove(index);
        } else {
            if track_split {
                self.tracks[index].is_original_a = false;
            }
            self.tracks[index].is_original_b = false;
            self.tracks[index].tag_b = None;
            if track_split {
                self.tracks[index].tag_real = None;
            }
        }
    }

    // Interrupt all tracks up the ancestry until we get to
    // a particular type, OR a type than could be an ancestor
    // of the given type
    fn interrupt(&mut self, itype: TrackType, inclusive: bool) {
        let mut regen = vec![];
        while let Some(track) = self.current() {
            let (istag, hasparent) = if let Some(ref real) = track.tag_real {
                println!("WOW {:?} {:?}", real, itype);
                let tag_type = real.tag_type().unwrap();
                (
                    tag_type == itype,
                    tag_type.ancestors().iter().any(|x| *x == itype),
                )
            } else {
                (false, false)
            };
            if track.tag_real.is_some() && ((!istag && hasparent) || (istag && inclusive)) {
                // schema.findType(tran.current()[1]) != type && schema.getAncestors(type).indexOf(schema.findType(tran.current()[1])) == -1
                println!("aborting by {:?} {:?} {:?}", itype, inclusive, istag);
                let aborted = self.abort();
                regen.push(aborted);
                if istag && inclusive {
                    break;
                }
            } else {
                break;
            }
        }

        for group in regen {
            self.tracks.push(Track {
                tag_a: group.0,
                tag_real: None,
                tag_b: group.2,
                is_original_a: false,
                is_original_b: false,
            })
        }
    }

    fn regenerate(&mut self) {
        // okay do regen
        // Filter for types that are ancestors of the current type.
        // TODO
        for track in &mut self.tracks {
            if track.tag_real.is_none() {
                println!("REGENERATE: {:?}", track);
                if let (&Some(ref a), &Some(ref b)) = (&track.tag_a, &track.tag_b) {
                    track.tag_real = Some(get_real_merge(a, b).unwrap_or_else(|| a.clone()));
                    track.is_original_a = false;
                    track.is_original_b = false;
                } else if track.tag_b.is_some() {
                    track.tag_real = track.tag_b.clone();
                // track.tag_a = track.tag_b.clone();
                // track.is_original_b = false;

                // if (origA) {
                //   insrA.enter();
                // } else {
                //   insrA.open(a || b, {});
                // }

                // if (origB) {
                //   insrB.enter();
                // } else {
                //   insrB.open(a || b, {});
                // }
                } else if track.tag_a.is_some() {
                    track.tag_real = track.tag_a.clone();
                    // track.tag_a = track.tag_a.clone();
                    // track.is_original_a = false;
                }

                // This only happens when opening split elements.
                // if !track.is_original_a {
                self.a_add.begin();
                // }
                // if !track.is_original_b {
                self.b_add.begin();
                // }
            }
        }
    }

    fn regenerate_until(&mut self, target: &TrackType) {
        // okay do regen
        // Filter for types that are ancestors of the current type.
        // TODO
        for track in &mut self.tracks {
            if track.tag_real.is_none() {
                println!("REGENERATE UNTIL: {:?}", target);

                let track_type = track._anything().unwrap().tag_type().unwrap();
                if target.parents().iter().position(|x| *x == track_type).is_none()
                    && *target != track_type {
                    println!("met {:?}", target);
                    break;
                }

                if let (&Some(ref a), &Some(ref b)) = (&track.tag_a, &track.tag_b) {
                    track.tag_real = Some(get_real_merge(a, b).unwrap_or_else(|| a.clone()));
                    track.is_original_a = false;
                    track.is_original_b = false;
                } else if track.tag_b.is_some() {
                    track.tag_real = track.tag_b.clone();
                // track.tag_a = track.tag_b.clone();
                // track.is_original_b = false;

                // if (origA) {
                //   insrA.enter();
                // } else {
                //   insrA.open(a || b, {});
                // }

                // if (origB) {
                //   insrB.enter();
                // } else {
                //   insrB.open(a || b, {});
                // }
                } else if track.tag_a.is_some() {
                    track.tag_real = track.tag_a.clone();
                    // track.tag_a = track.tag_a.clone();
                    // track.is_original_a = false;
                }

                // This only happens when opening split elements.
                // if !track.is_original_a {
                self.a_add.begin();
                // }
                // if !track.is_original_b {
                self.b_add.begin();
                // }
            }
        }
    }

    fn result(mut self) -> (Op, Op) {
        let mut a_del = self.a_del;
        let mut a_add = self.a_add;
        let mut b_del = self.b_del;
        let mut b_add = self.b_add;

        for track in self.tracks.iter_mut().rev() {
            println!("TRACK RESULT: {:?}", track);
            if !track.is_original_a && track.tag_real.is_some() {
                a_add.close(track.tag_a.clone().unwrap().to_attrs());
            }
            if track.is_original_a {
                a_del.exit();
                a_add.exit();
            }
            if !track.is_original_b && track.tag_real.is_some() {
                b_add.close(track.tag_b.clone().unwrap().to_attrs());
            }
            if track.is_original_b {
                b_del.exit();
                b_add.exit();
            }
        }
        ((a_del.result(), a_add.result()), (
            b_del.result(),
            b_add.result(),
        ))
    }

    fn current_type(&self) -> Option<TrackType> {
        // TODO
        // self.tracks.last().unwrap().
        let attrs = self.tracks
            .last()
            .unwrap()
            .tag_real
            .clone()
            .unwrap()
            .to_attrs();
        Tag::from_attrs(&attrs).tag_type()
    }

    fn supports_text(&self) -> bool {
        if let Some((track, _)) = self.top_track_real() {
            // TODO also inlines TODO also move logic to schema
            if track.tag_real.unwrap().tag_type() == Some(TrackType::Blocks) {
                return true;
            }
        }
        false
    }
}

pub fn transform_insertions(avec: &AddSpan, bvec: &AddSpan) -> (Op, Op) {
    let mut a = AddStepper::new(avec);
    let mut b = AddStepper::new(bvec);

    let mut t = Transform::new();

    while !(a.is_done() && b.is_done()) {
        println!("{}", Green.bold().paint("Tracks:"));
        for t in &t.tracks {
            println!("{}", BrightGreen.paint(format!(" - {:?}", t)));
        }

        println!("{}", BrightGreen.paint(format!(" @ a_del: {:?}", t.a_del)));
        println!("{}", BrightGreen.paint(format!(" @ a_add: {:?}", t.a_add)));
        println!("{}", BrightGreen.paint(format!(" @ b_del: {:?}", t.b_del)));
        println!("{}", BrightGreen.paint(format!(" @ b_add: {:?}", t.b_add)));

        if a.is_done() {
            // println!("tracks {:?}", t.tracks);
            t.regenerate();
            println!(
                "{}",
                BrightYellow.paint(format!("Finishing B: {:?}", b.head.clone()))
            );

            match b.head.clone() {
                Some(AddGroup(ref attrs, ref span)) => {
                    t.skip_b(1);
                    t.group_a(attrs, span);
                    b.next();
                }
                Some(AddWithGroup(ref span)) => {
                    t.skip_b(1);
                    t.with_group_a(span);
                    b.next();
                }
                Some(AddChars(ref b_chars)) => {
                    t.chars_a(b_chars);
                    t.skip_b(b_chars.len());
                    b.next();
                }
                Some(AddSkip(b_count)) => {
                    t.skip_a(b_count);
                    t.skip_b(b_count);
                    b.next();
                }
                None => {
                    t.close_b();
                    b.exit();
                }
            }
        } else if b.is_done() {
            t.regenerate();
            println!(
                "{}",
                BrightYellow.paint(format!("Finishing A (add): {:?}", a.head.clone()))
            );

            match a.head.clone() {
                Some(AddGroup(ref attrs, ref span)) => {
                    t.skip_a(1);
                    t.group_b(attrs, span);
                    a.next();
                }
                Some(AddWithGroup(ref span)) => {
                    t.skip_a(1);
                    t.with_group_b(span);
                    a.next();
                }
                Some(AddChars(ref a_chars)) => {
                    t.skip_a(a_chars.len());
                    t.chars_b(a_chars);
                    a.next();
                }
                Some(AddSkip(a_count)) => {
                    t.skip_a(a_count);
                    t.skip_b(a_count);
                    a.next();
                }
                None => {
                    t.close_a();
                    a.exit();
                }
            }
        } else {
            println!(
                "{}",
                BrightYellow.paint(format!("Next step (ins): {:?}", (a.head.clone(), b.head.clone())))
            );

            match (a.head.clone(), b.head.clone()) {
                // Closing
                (None, None) => {
                    let (a_tag, b_tag) = {
                        let t = t.tracks.last().unwrap();
                        (t.tag_a.clone(), t.tag_b.clone())
                    };

                    if a_tag.is_some() && b_tag.is_some() &&
                        a_tag.clone().unwrap().tag_type() == b_tag.clone().unwrap().tag_type()
                    {
                        // t.interrupt(a_tag || b_tag);
                        a.exit();
                        b.exit();
                        t.close();
                    } else if a_tag.is_some() &&
                               (b_tag.is_none() ||
                                    a_tag
                                        .clone()
                                        .unwrap()
                                        .tag_type()
                                        .unwrap()
                                        .ancestors()
                                        .iter()
                                        .any(|x| *x == b_tag.clone().unwrap().tag_type().unwrap()))
                    {
                        // t.interrupt(a_tag);
                        a.exit();
                        t.close_a();
                    } else if b_tag.is_some() {
                        // t.interrupt(b_tag);
                        b.exit();
                        t.close_b();
                    }
                }
                (Some(AddChars(ref a_chars)), None) => {
                    t.regenerate();

                    t.skip_a(a_chars.len());
                    t.chars_b(a_chars);
                    a.next();
                }

                // TODO don't like that this isn't a pattern match;
                // This case should handle AddWithGroup and AddGroup (I believe)
                (None, compare) => {
                    let ok = if let Some(AddChars(ref b_chars)) = compare {
                        if t.supports_text() {
                            t.chars_a(b_chars);
                            t.skip_b(b_chars.chars().count());
                            b.next();
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // TODO this logic is evidence AddObject should be broken out
                    let groupsuccess = if let Some(AddGroup(ref b_attrs, _)) = compare {
                        if b_attrs["tag"] == "caret" && t.supports_text() {
                            b.enter();
                            b.exit();
                            t.enter_b(None, Tag::from_attrs(b_attrs));
                            t.close_b();

                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !ok && !groupsuccess {
                        let a_typ = t.tracks
                            .iter()
                            .rev()
                            .find(|t| t.tag_a.is_some())
                            .unwrap()
                            .tag_a
                            .clone()
                            .unwrap()
                            .tag_type()
                            .unwrap();
                        println!("what is up with a {:?}", t.a_add);
                        t.interrupt(a_typ, false);
                        // println!("... {:?} {:?}", t.a_del, t.a_add);
                        // println!("... {:?} {:?}", t.b_del, t.b_add);
                        println!("~~~> tracks {:?}", t.tracks);
                        t.close_a();
                        // println!("...");
                        a.exit();
                        println!("<~~~ tracks {:?}", t.tracks);
                        // println!("WHERE ARE WE WITH A {:?}", a);
                    }
                }

                // caret-42
                // (None, Some(AddChars(ref b_chars))) => {
                //     t.chars_a(b_chars);
                //     t.skip_b(b_chars.len());
                //     b.next();
                // }

                // Opening
                (Some(AddGroup(ref a_attrs, _)), Some(AddGroup(ref b_attrs, _))) => {
                    // TODO should t.regenerate be called??

                    let a_type = Tag::from_attrs(a_attrs).tag_type().unwrap();
                    let b_type = Tag::from_attrs(b_attrs).tag_type().unwrap();

                    let b_is_child_of_a =
                        Tag::from_attrs(b_attrs)
                            .tag_type()
                            .unwrap()
                            .ancestors()
                            .iter()
                            .any(|x| *x == Tag::from_attrs(a_attrs).tag_type().unwrap());

                    println!("groupgruop {:?} {:?}", a_type, b_type);
                    if a_attrs["tag"] == "caret" && b_attrs["tag"] == "caret" {
                        t.regenerate();
                        // Carets
                        a.enter();
                        a.exit();
                        b.enter();
                        b.exit();
                        t.enter_a(&Tag::from_attrs(a_attrs), None);
                        t.close_a();
                        t.enter_b(None, Tag::from_attrs(b_attrs));
                        t.close_b();
                    } else if a_type == b_type {
                        a.enter();
                        b.enter();
                        if Tag::from_attrs(a_attrs) == Tag::from_attrs(b_attrs) {
                            t.enter(&Tag::from_attrs(a_attrs));
                        } else {
                            t.enter_a(&Tag::from_attrs(a_attrs), Some(Tag::from_attrs(b_attrs)));
                        }
                    } else if b_is_child_of_a {
                        let a_type = Tag::from_attrs(a_attrs).tag_type();
                        t.regenerate_until(&a_type.clone().unwrap());

                        a.enter();

                        println!("~~~~ :O");
                        println!("~~~~ -> {:?} {:?}", t.next_track_a_type(), a_type);
                        if t.next_track_a_by_type(a_type.unwrap()).is_some() {
                            // if a_type.map_or(false, |x| x.do_open_split()) {
                            if true {
                                println!("INTERRUPTING A");
                                t.interrupt(a_type.unwrap(), false);
                                println!("BUT THE TRACKS -----<> {:?}", t.tracks);
                                if let Some(j) = t.next_track_a_by_type(a_type.unwrap()) {
                                    j.tag_a = Some(Tag::from_attrs(a_attrs));
                                    j.is_original_a = false;
                                    println!("inject A");
                                }
                                t.a_del.begin();
                            } else {
                                t.unenter_a();
                            }
                        } else {
                            t.interrupt(a_type.unwrap(), false); // caret-46
                            t.enter_a(&Tag::from_attrs(a_attrs), None);
                        }
                    } else {
                        // println!("groupgruop {:?} {:?}", a_type, b_type);
                        let b_type = Tag::from_attrs(b_attrs).tag_type();
                        t.regenerate_until(&b_type.clone().unwrap());

                        b.enter();

                        // println!("TELL ME {:?} {:?}", t.next_track_by_type(b_type.unwrap()), b_type);

                        if t.next_track_b_by_type(b_type.unwrap()).is_some() {
                            // if b_type.map_or(false, |x| x.do_open_split()) {
                            if true {
                                println!("INTERRUPTING B");
                                t.interrupt(b_type.unwrap(), false);
                                if let Some(j) = t.next_track_b_by_type(b_type.unwrap()) {
                                    j.tag_b = Some(Tag::from_attrs(b_attrs));
                                    j.is_original_b = false;
                                    println!("inject B");
                                }
                                t.b_del.begin();
                            } else {
                                t.unenter_b(b_type.unwrap());
                            }
                        } else {
                            t.interrupt(b_type.unwrap(), false); // caret-43
                            t.enter_b(None, Tag::from_attrs(b_attrs));
                        }
                    }

                    // TODO if they are different tags THEN WHAT
                }
                (compare, None) => {
                    // println!("... {:?} {:?}", t.a_del, t.a_add);
                    // println!("... {:?} {:?}", t.b_del, t.b_add);

                    // TODO this logic is evidence AddObject should be broken out
                    let groupsuccess = if let Some(AddGroup(ref a_attrs, _)) = compare {
                    t.regenerate(); // TODO is this correct
                        if a_attrs["tag"] == "caret" && t.supports_text() {
                            a.enter();
                            a.exit();
                            t.enter_a(&Tag::from_attrs(a_attrs), None);
                            t.close_a();

                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !groupsuccess {
                        let b_typ = t.tracks
                            .iter()
                            .rev()
                            .find(|t| t.tag_b.is_some())
                            .unwrap()
                            .tag_b
                            .clone()
                            .unwrap()
                            .tag_type()
                            .unwrap();
                        t.interrupt(b_typ, false);
                        t.close_b();
                        // t.closeA()
                        b.exit();
                    }
                }
                (Some(AddGroup(ref a_attrs, _)), _) => {
                    // TODO should carets be worked around like this?

                    let a_type = Tag::from_attrs(a_attrs).tag_type();
                    t.regenerate_until(&a_type.clone().unwrap());

                    // TODO should t.regenerate be called??
                    if a_attrs["tag"] == "caret" {
                        // Carets
                        a.enter();
                        a.exit();
                        t.enter_a(&Tag::from_attrs(a_attrs), None);
                        t.close_a();
                    } else {
                        a.enter();

                        println!("~~~~ :) :) :)");
                        println!("~~~~ -> {:?} {:?}", t.next_track_a_by_type(a_type.unwrap()), a_type);
                        // in/15, caret-34
                        // if t.next_track_a_type() == a_type {
                        if t.next_track_a_by_type(a_type.unwrap()).is_some() {
                            if a_type.map_or(false, |x| x.do_open_split()) {
                            // if true {
                                println!("INTERRUPTING A");
                                t.interrupt(a_type.unwrap(), true);
                                if let Some(j) = t.next_track_a() {
                                    j.tag_a = Some(Tag::from_attrs(a_attrs));
                                    j.is_original_a = true;
                                }
                                t.a_del.begin();
                            } else {
                                t.unenter_a();
                            }
                        } else {
                            t.interrupt(a_type.unwrap(), true);
                            t.enter_a(&Tag::from_attrs(a_attrs), None);
                        }
                    }

                    // println!("adding left group:");
                    // for t in &t.tracks {
                    //     println!(" - {:?}", t);
                    // }
                }
                (_, Some(AddGroup(ref b_attrs, _))) => {// TODO should carets be worked around like this?
                    // TODO should t.regenerate be called??
                    let b_type = Tag::from_attrs(b_attrs).tag_type();
                    t.regenerate_until(&b_type.clone().unwrap());
                    println!("TRACKS {:?}", t.tracks);

                    if b_attrs["tag"] == "caret" {
                        // Carets
                        b.enter();
                        b.exit();
                        t.enter_b(None, Tag::from_attrs(b_attrs));
                        t.close_b();
                    } else {
                        // println!("groupgruop {:?} {:?}", a_type, b_type);
                        b.enter();
                        // let b_type = Tag::from_attrs(b_attrs).tag_type();

                        if t.next_track_b_by_type(b_type.unwrap()).is_some() {
                            if b_type.map_or(false, |x| x.do_open_split()) {
                                println!("INTERRUPTING B");
                                t.interrupt(b_type.unwrap(), true);
                                if let Some(j) = t.next_track_b() {
                                    j.tag_b = Some(Tag::from_attrs(b_attrs));
                                    j.is_original_b = true;
                                }
                                t.b_del.begin();
                            } else {
                                t.unenter_b(b_type.unwrap());
                            }
                        } else {
                            t.interrupt(b_type.unwrap(), false); // caret-32
                            t.enter_b(None, Tag::from_attrs(b_attrs));
                        }
                    }
                }

                // Rest
                (Some(AddSkip(a_count)), Some(AddSkip(b_count))) => {
                    t.regenerate();

                    if a_count > b_count {
                        a.head = Some(AddSkip(a_count - b_count));
                        b.next();
                    } else if a_count < b_count {
                        a.next();
                        b.head = Some(AddSkip(b_count - a_count));
                    } else {
                        a.next();
                        b.next();
                    }
                    t.skip_a(cmp::min(a_count, b_count));
                    t.skip_b(cmp::min(a_count, b_count));
                }
                (Some(AddSkip(a_count)), Some(AddChars(ref b_chars))) => {
                    t.regenerate();

                    b.next();
                    t.chars_a(b_chars);
                    t.skip_b(b_chars.chars().count());
                }
                (Some(AddChars(ref a_chars)), _) => {
                    t.regenerate();

                    t.skip_a(a_chars.chars().count());
                    t.chars_b(a_chars);
                    a.next();
                }

                // With Groups
                (Some(AddWithGroup(a_inner)), Some(AddSkip(b_count))) => {
                    t.regenerate(); // caret-31

                    t.a_del.skip(1);
                    t.a_add.skip(1);
                    t.b_del.skip(1);
                    t.b_add.with_group(&a_inner);

                    a.next();
                    if b_count > 1 {
                        b.head = Some(AddSkip(b_count - 1));
                    } else {
                        b.next();
                    }
                }
                (Some(AddWithGroup(a_inner)), Some(AddWithGroup(b_inner))) => {
                    t.regenerate(); // caret-31

                    let (a_op, b_op) = transform_insertions(&a_inner, &b_inner);

                    t.a_del.with_group(&a_op.0);
                    t.a_add.with_group(&a_op.1);
                    t.b_del.with_group(&b_op.0);
                    t.b_add.with_group(&b_op.1);

                    a.next();
                    b.next();
                }
                (Some(AddSkip(a_count)), Some(AddWithGroup(b_inner))) => {
                    t.regenerate(); // caret-31

                    t.a_del.skip(1);
                    t.a_add.with_group(&b_inner);
                    t.b_del.skip(1);
                    t.b_add.skip(1);

                    if a_count > 1 {
                        a.head = Some(AddSkip(a_count - 1));
                    } else {
                        a.next();
                    }
                    b.next();
                }

                // Invalid
                // TODO not invalid; is this right now?
                (Some(AddWithGroup(ref a_inner)), Some(AddChars(ref b_chars))) => {
                    t.regenerate(); // caret-35

                    t.chars_a(b_chars);
                    // t.a_del.skip(1);
                    // t.a_add.skip(1);
                    t.b_del.skip(b_chars.chars().count());
                    t.b_add.skip(b_chars.chars().count());
                    // t.b_add.with_group(&a_inner);

                    // a.next();
                    b.next();
                }
            }
        }
    }

    println!("TRACK A DEL {:?}", t.a_del);
    println!("TRACK A ADD {:?}", t.a_add);
    println!("TRACK B DEL {:?}", t.b_del);
    println!("TRACK B ADD {:?}", t.b_add);

    let (op_a, op_b) = t.result();
    println!("RESULT A: {:?}", op_a.clone());
    println!("RESULT B: {:?}", op_b.clone());
    (op_a, op_b)
}


fn normalize_delgroupall_element(elem: DelElement) -> DelElement {
    match elem {
        DelGroup(span) => {
            if span.len() > 0 && span.skip_post_len() == 0 {
                println!("---> {:?}", span);
                println!("what {:?}", span.skip_post_len());
                DelGroupAll
            } else {
                DelGroup(normalize_delgroupall(span))
            }
        }
        DelWithGroup(span) => {
            DelWithGroup(normalize_delgroupall(span))
        }
        _ => elem,
    }
}

pub fn normalize_delgroupall(del: DelSpan) -> DelSpan {
    let mut ret: DelSpan = vec![];
    for elem in del.into_iter() {
        ret.place(&normalize_delgroupall_element(elem));
    }
    ret
}

pub fn transform_deletions(avec: &DelSpan, bvec: &DelSpan) -> (DelSpan, DelSpan) {
    let mut a = DelStepper::new(avec);
    let mut b = DelStepper::new(bvec);

    let mut a_del = DelWriter::new();
    let mut b_del = DelWriter::new();

    while !(a.is_done() && b.is_done()) {
        println!("{}", Green.bold().paint("transform_deletions:"));
        println!("{}", BrightGreen.paint(format!(" @ a_del: {:?}", a_del)));
        println!("{}", BrightGreen.paint(format!(" @ b_del: {:?}", b_del)));

        if a.is_done() {
            println!(
                "{}",
                BrightYellow.paint(format!("Finishing B: {:?}", b.head.clone()))
            );

            match b.head.clone() {
                Some(DelObject) => {
                    unimplemented!();
                }
                Some(DelGroup(ref span)) => {
                    // t.skip_b(1);
                    // t.group_a(attrs, span);
                    a_del.group(span);
                    b.next();
                }
                Some(DelWithGroup(ref span)) => {
                    a_del.with_group(span);
                    b.next();
                }
                Some(DelChars(b_chars)) => {
                    // t.chars_a(b_chars);
                    // t.skip_b(b_chars.len());
                    a_del.chars(b_chars);
                    b.next();
                }
                Some(DelSkip(b_count)) => {
                    // t.skip_a(b_count);
                    // t.skip_b(b_count);
                    a_del.skip(b_count);
                    b.next();
                }
                Some(DelGroupAll) => {
                    a_del.group_all();
                    b.next();
                }
                None => {
                    // t.close_b();
                    b.exit();
                }
            }
        } else if b.is_done() {
            println!(
                "{}",
                BrightYellow.paint(format!("Finishing A (del): {:?}", a.head.clone()))
            );

            match a.head.clone() {
                Some(DelObject) => {
                    unimplemented!();
                }
                Some(DelGroup(ref span)) => {
                    // t.skip_a(1);
                    // t.group_b(attrs, span);
                    // a_del.skip(1);
                    b_del.group(span);
                    a.next();
                }
                Some(DelWithGroup(ref span)) => {
                    // t.skip_a(1);
                    // t.group_b(attrs, span);
                    b_del.with_group(span);
                    a_del.skip(1);
                    a.next();
                }
                Some(DelChars(a_chars)) => {
                    // t.skip_a(a_chars.len());
                    // t.chars_b(a_chars);
                    b_del.chars(a_chars);
                    a.next();
                }
                Some(DelSkip(a_count)) => {
                    // t.skip_a(a_count);
                    // t.skip_b(a_count);
                    a.next();
                    b_del.skip(a_count);
                }
                Some(DelGroupAll) => {
                    a.next();
                    b_del.group_all();
                }
                None => {
                    // t.close_a();
                    a.exit();
                }
            }
        } else {
            println!(
                "{}",
                BrightYellow.paint(format!("Next step (del): {:?}", (a.head.clone(), b.head.clone())))
            );

            match (a.head.clone(), b.head.clone()) {
                // Groups
                (Some(DelGroup(a_inner)), Some(DelGroup(b_inner))) => {
                    let (a_del_inner, b_del_inner) = transform_deletions(&a_inner, &b_inner);

                    a_del.place_all(&a_del_inner);
                    b_del.place_all(&b_del_inner);

                    a.next();
                    b.next();
                }
                (Some(DelGroup(a_inner)), Some(DelWithGroup(b_inner))) => {
                    let (a_del_inner, b_del_inner) = transform_deletions(&a_inner, &b_inner);

                    a_del.place_all(&a_del_inner);
                    b_del.group(&b_del_inner);

                    a.next();
                    b.next();
                }
                (Some(DelSkip(a_count)), Some(DelGroup(b_inner))) => {
                    a_del.group(&b_inner);
                    if a_count > 1 {
                        a.head = Some(DelSkip(a_count - 1));
                    } else {
                        a.next();
                    }

                    if b_inner.skip_len() > 0 {
                        b_del.skip(b_inner.skip_post_len());
                    }
                    b.next();
                }
                (Some(DelSkip(a_count)), Some(DelGroupAll)) => {
                    a_del.group_all();
                    if a_count > 1 {
                        a.head = Some(DelSkip(a_count - 1));
                    } else {
                        a.next();
                    }

                    b.next();
                }
                (Some(DelGroup(a_inner)), Some(DelSkip(b_count))) => {
                    if a_inner.skip_post_len() > 0 {
                        a_del.skip(a_inner.skip_post_len());
                    }
                    b_del.group(&a_inner);

                    a.next();
                    if b_count > 1 {
                        b.head = Some(DelSkip(b_count - 1));
                    } else {
                        b.next();
                    }
                }

                // Rest
                (Some(DelSkip(a_count)), Some(DelSkip(b_count))) => {
                    if a_count > b_count {
                        a.head = Some(DelSkip(a_count - b_count));
                        b.next();
                    } else if a_count < b_count {
                        a.next();
                        b.head = Some(DelSkip(b_count - a_count));
                    } else {
                        a.next();
                        b.next();
                    }

                    a_del.skip(cmp::min(a_count, b_count));
                    b_del.skip(cmp::min(a_count, b_count));
                }
                (Some(DelSkip(a_count)), Some(DelChars(b_chars))) => {
                    if a_count > b_chars {
                        a.head = Some(DelSkip(a_count - b_chars));
                        b.next();
                        a_del.chars(b_chars);
                    } else if a_count < b_chars {
                        a.next();
                        b.head = Some(DelChars(b_chars - a_count));
                        a_del.chars(a_count);
                    } else {
                        a.next();
                        b.next();
                        a_del.chars(b_chars);
                    }
                }
                (Some(DelChars(a_chars)), Some(DelChars(b_chars))) => {
                    if a_chars > b_chars {
                        a.head = Some(DelChars(a_chars - b_chars));
                        b.next();
                    } else if a_chars < b_chars {
                        a.next();
                        b.head = Some(DelChars(b_chars - a_chars));
                    } else {
                        a.next();
                        b.next();
                    }
                }
                (Some(DelChars(a_chars)), Some(DelSkip(b_count))) => {
                    if a_chars > b_count {
                        a.head = Some(DelChars(a_chars - b_count));
                        b.next();
                    } else if a_chars < b_count {
                        a.next();
                        b.head = Some(DelSkip(b_count - a_chars));
                    } else {
                        a.next();
                        b.next();
                    }

                    // a_del.skip(cmp::min(a_chars, b_chars));
                    b_del.chars(cmp::min(a_chars, b_count));
                }
                (Some(DelChars(a_chars)), _) => {
                    a.next();
                    b_del.chars(a_chars);
                }

                // With Groups
                (Some(DelWithGroup(a_inner)), Some(DelWithGroup(b_inner))) => {
                    let (a_del_inner, b_del_inner) = transform_deletions(&a_inner, &b_inner);

                    a_del.with_group(&a_del_inner);
                    b_del.with_group(&b_del_inner);

                    a.next();
                    b.next();
                }
                (Some(DelSkip(a_count)), Some(DelWithGroup(b_inner))) => {
                    a_del.with_group(&b_inner);
                    b_del.skip(1);

                    if a_count > 1 {
                        a.head = Some(DelSkip(a_count - 1));
                    } else {
                        a.next();
                    }
                    b.next();
                }
                (Some(DelWithGroup(a_inner)), Some(DelSkip(b_count))) => {
                    a_del.skip(1);
                    b_del.with_group(&a_inner);

                    a.next();
                    if b_count > 1 {
                        b.head = Some(DelSkip(b_count - 1));
                    } else {
                        b.next();
                    }
                }

                // DelGroupAll
                (Some(DelGroupAll), Some(DelWithGroup(_))) => {
                    b_del.group_all();

                    a.next();
                    b.next();
                }
                (Some(DelWithGroup(_)), Some(DelGroupAll)) => {
                    a_del.group_all();

                    a.next();
                    b.next();
                }
                (Some(DelWithGroup(a_inner)), Some(DelGroup(b_inner))) => {
                    let (a_del_inner, b_del_inner) = transform_deletions(&a_inner, &b_inner);

                    a_del.group(&a_del_inner);
                    b_del.place_all(&b_del_inner);

                    a.next();
                    b.next();
                }
                (Some(DelGroupAll), Some(DelSkip(b_count))) => {
                    b_del.group_all();

                    a.next();
                    if b_count > 1 {
                        b.head = Some(DelSkip(b_count - 1));
                    } else {
                        b.next();
                    }
                }
                // TODO
                // (Some(DelGroupAll), Some(DelGroup(b_inner))) => {
                //     b_del.group_all();

                //     a.next();
                //     b.next();
                // }

                unimplemented => {
                    println!("Not reachable: {:?}", unimplemented);
                    unreachable!();
                }
            }
        }
    }

    let a_res = a_del.result();
    let b_res = b_del.result();

    println!("{}", BrightYellow.paint(format!("Result A: {:?}", a_res)));
    println!("{}", BrightYellow.paint(format!("Result B: {:?}", b_res)));

    (a_res, b_res)
}

/// Transforms a insertion preceding a deletion into a deletion preceding an insertion.
/// After this, sequential deletions and insertions can be composed together in one operation.

/*
function delAfterIns (insA, delB, schema) {
  var c, delr, insr, iterA, iterB, _ref;
  _ref = oatie.record(), delr = _ref[0], insr = _ref[1];
  iterA = new oatie.OpIterator(insA);
  iterB = new oatie.OpIterator(delB);
  iterA.next();
  iterB.next();
  c = oatie._combiner([delr, insr], iterA, true, iterB, false);
  c.delAfterIns = function () {
    var _ref1, _ref2, _ref3, _ref4, _ref5, _ref6, _ref7, _ref8, _ref9;
    while (!(iterA.isExit() || iterB.isExit())) {
      if ((_ref1 = iterA.type) === 'text') {
        c.useA();
        delr.retain();
      }
      if ((_ref2 = iterA.type) === 'open') {
        delr.enter(iterA.tag, iterA.attrs);
        c.useA();
      }
      if (iterA.type === 'retain' && (iterB.type === 'remove' || iterB.type === 'retain')) {
        c.pickB();
      } else if (iterA.type === 'retain' && (iterB.type === 'unopen' || iterB.type === 'enter')) {
        c.nextA().consumeB();
      } else if (((_ref6 = iterA.type) === 'enter' || _ref6 === 'attrs') && (iterB.type === 'remove')) {
        c.retainA();
      } else if (((_ref6 = iterA.type) === 'enter' || _ref6 === 'attrs') && (iterB.type === 'retain')) {
        c.retainA().nextB();
      } else if (((_ref8 = iterA.type) === 'enter' || _ref8 === 'attrs') && ((_ref9 = iterB.type) === 'enter' || _ref9 === 'unopen')) {
        c.pickB().delAfterIns().pickB();
      }
    }

    while (!iterA.isExit()) {
      c.retainA();
    }
    // Catch .close() ending.
    if (iterA.type == 'close') {
      delr.leave();
      c.useA();
      return this.delAfterIns();
    }
    while (!iterB.isExit()) {
      if (iterB.type === 'retain') {
        c.useB();
      } else {
        c.consumeB();
      }
    }
    return this;
  };
  return c.delAfterIns().toJSON();
}
*/

pub fn transform_add_del_inner(
    delres: &mut DelSpan,
    addres: &mut AddSpan,
    a: &mut AddStepper,
    b: &mut DelStepper,
) {
    while !b.is_done() && !a.is_done() {
        println!("-----> {:?} {:?}", a.head, b.head);


        match b.get_head() {
            DelObject => {
                unimplemented!();
            }
            DelChars(bcount) => {
                match a.get_head() {
                    AddChars(avalue) => {
                        addres.place(&AddChars(avalue.clone()));
                        delres.place(&DelSkip(avalue.len()));
                        a.next();
                    }
                    AddSkip(acount) => {
                        if bcount < acount {
                            a.head = Some(AddSkip(acount - bcount));
                            delres.place(&b.next().unwrap());
                        } else if bcount > acount {
                            a.next();
                            delres.place(&DelChars(acount));
                            b.head = Some(DelChars(bcount - acount));
                        } else {
                            a.next();
                            delres.place(&b.next().unwrap());
                        }
                    }
                    AddGroup(attrs, a_span) => {
                        let mut a_inner = AddStepper::new(&a_span);
                        let mut addres_inner: AddSpan = vec![];
                        let mut delres_inner: DelSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            b,
                        );
                        if !a_inner.is_done() {
                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }
                        addres.place(&AddGroup(attrs, addres_inner));
                        delres.place(&DelWithGroup(delres_inner));
                        a.next();
                    }
                    unknown => {
                        println!("Compare: {:?} {:?}", DelChars(bcount), unknown);
                        panic!("Unimplemented or Unexpected");
                    }
                }
            }
            DelSkip(bcount) => {
                match a.get_head() {
                    AddChars(avalue) => {
                        addres.place(&AddChars(avalue.clone()));
                        delres.place(&DelSkip(avalue.len()));
                        a.next();
                    }
                    AddSkip(acount) => {
                        addres.place(&AddSkip(cmp::min(acount, bcount)));
                        delres.place(&DelSkip(cmp::min(acount, bcount)));
                        if acount > bcount {
                            a.head = Some(AddSkip(acount - bcount));
                            b.next();
                        } else if acount < bcount {
                            a.next();
                            b.head = Some(DelSkip(bcount - acount));
                        } else {
                            a.next();
                            b.next();
                        }
                    }
                    AddWithGroup(..) => {
                        addres.place(&a.next().unwrap());
                        delres.place(&DelSkip(1));
                        if bcount == 1 {
                            b.next();
                        } else {
                            b.head = Some(DelSkip(bcount - 1));
                        }
                    }
                    AddGroup(attrs, a_span) => {
                        let mut a_inner = AddStepper::new(&a_span);
                        let mut addres_inner: AddSpan = vec![];
                        let mut delres_inner: DelSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            b,
                        );
                        if !a_inner.is_done() {
                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }
                        addres.place(&AddGroup(attrs, addres_inner));
                        delres.place(&DelWithGroup(delres_inner));
                        a.next();
                    }
                }
            }
            DelWithGroup(span) => {
                match a.get_head() {
                    AddChars(avalue) => {
                        delres.place(&DelSkip(avalue.chars().count()));
                        addres.place(&a.next().unwrap());
                    }
                    AddSkip(acount) => {
                        delres.place(&b.next().unwrap());
                        addres.place(&AddSkip(1));
                        if acount > 1 {
                            a.head = Some(AddSkip(acount - 1));
                        } else {
                            a.next();
                        }
                    }
                    AddWithGroup(insspan) => {
                        a.next();
                        b.next();

                        let (del, ins) = transform_add_del(&insspan, &span);
                        delres.place(&DelWithGroup(del));
                        addres.place(&AddWithGroup(ins));
                    }
                    AddGroup(attrs, a_span) => {
                        let mut a_inner = AddStepper::new(&a_span);
                        let mut addres_inner: AddSpan = vec![];
                        let mut delres_inner: DelSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            b,
                        );
                        if !a_inner.is_done() {
                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }
                        addres.place(&AddGroup(attrs, addres_inner));
                        delres.place(&DelWithGroup(delres_inner));
                        a.next();

                        // a.next();
                        // b.next();

                        // let (_, ins) = transform_add_del(&insspan, &span);
                        // addres.place(&AddGroup(attr, ins));
                    }
                }
            }
            DelGroup(span) => {
                match a.get_head() {
                    AddChars(avalue) => {
                        delres.place(&DelSkip(avalue.chars().count()));
                        addres.place(&a.next().unwrap());
                    }
                    AddSkip(acount) => {
                        delres.place(&b.next().unwrap());
                        if span.skip_post_len() > 0 {
                            addres.place(&AddSkip(span.skip_post_len()));
                        }
                        if acount > 1 {
                            a.head = Some(AddSkip(acount - 1));
                        } else {
                            a.next();
                        }
                    }





                    AddWithGroup(ins_span) => {
                        let mut a_inner = AddStepper::new(&ins_span);
                        let mut b_inner = DelStepper::new(&span);
                        let mut delres_inner: DelSpan = vec![];
                        let mut addres_inner: AddSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            &mut b_inner,
                        );

                        // TODO should this be part of the top-level resolution for transform_add_del
                        if !b_inner.is_done() {

                            if let &Some(ref head) = &b_inner.head {
                                let len = (vec![head.clone()]).skip_post_len();
                                if len > 0 {
                                    addres_inner.place(&AddSkip(len));
                                }
                            }
                            let len = b_inner.rest.skip_post_len();
                            if len > 0 {
                                addres_inner.place(&AddSkip(len));
                            }

                            delres_inner.place(&b_inner.head.unwrap());
                            delres_inner.place_all(&b_inner.rest);
                        } else if !a_inner.is_done() {

                            if let &Some(ref head) = &a_inner.head {
                                let len = (vec![head.clone()]).skip_len();
                                if len > 0 {
                                    delres_inner.place(&DelSkip(len));
                                }
                            }
                            let len = a_inner.rest.skip_len();
                            if len > 0 {
                                delres_inner.place(&DelSkip(len));
                            }

                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }

                        delres.place(&DelGroup(delres_inner));
                        addres.place_all(&addres_inner);

                        a.next();
                        b.next();
                    }
                        
                    // AddWithGroup(ins_span) => {
                    //     a.next();
                    //     b.next();

                    //     let (del, ins) = transform_add_del(&ins_span, &span);
                    //     delres.place(&DelGroup(del));
                    //     addres.place_all(&ins[..]);
                    // }
                    AddGroup(tags, ins_span) => {
                        let mut a_inner = AddStepper::new(&ins_span);
                        let mut delres_inner: DelSpan = vec![];
                        let mut addres_inner: AddSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            b,
                        );
                        if !a_inner.is_done() {
                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }
                        addres.place(&AddGroup(tags, addres_inner));
                        delres.place(&DelWithGroup(delres_inner));
                        a.next();
                    }
                }
            }
            DelGroupAll => {
                match a.get_head() {
                    AddChars(avalue) => {
                        delres.place(&DelSkip(avalue.chars().count()));
                        addres.place(&a.next().unwrap());
                    }
                    AddSkip(acount) => {
                        delres.place(&b.next().unwrap());
                        if acount > 1 {
                            a.head = Some(AddSkip(acount - 1));
                        } else {
                            a.next();
                        }
                    }
                    AddWithGroup(insspan) => {
                        a.next();
                        delres.place(&b.next().unwrap());
                    }
                    AddGroup(tags, ins_span) => {
                        let mut a_inner = AddStepper::new(&ins_span);
                        let mut delres_inner: DelSpan = vec![];
                        let mut addres_inner: AddSpan = vec![];
                        transform_add_del_inner(
                            &mut delres_inner,
                            &mut addres_inner,
                            &mut a_inner,
                            b,
                        );
                        if !a_inner.is_done() {
                            addres_inner.place(&a_inner.head.unwrap());
                            addres_inner.place_all(&a_inner.rest);
                        }
                        addres.place(&AddGroup(tags, addres_inner));
                        // delres.place(&DelGroupAll);
                        a.next();
                    }
                }
            }
        }
    }
}


pub fn transform_add_del(avec: &AddSpan, bvec: &DelSpan) -> Op {
    let mut delres: DelSpan = Vec::with_capacity(avec.len() + bvec.len());
    let mut addres: AddSpan = Vec::with_capacity(avec.len() + bvec.len());

    let mut a = AddStepper::new(avec);
    let mut b = DelStepper::new(bvec);

    transform_add_del_inner(&mut delres, &mut addres, &mut a, &mut b);

    if !b.is_done() {
        delres.place_all(&b.into_span());
    }

    if !a.is_done() {
        let rest = a.into_span();
        delres.place(&DelSkip(rest.skip_len()));
        addres.place_all(&rest);
    }

    (delres, addres)
}

/// Transform two operations according to a schema.

pub fn transform(a_original: &Op, b_original: &Op) -> (Op, Op) {
    // Transform deletions A and B against each other to get delA` and delB`.
    println!(" # transform[1] transform_deletions");
    println!(" a_del   {:?}", a_original.0);
    println!(" b_del   {:?}", b_original.0);
    println!();

    let mut a = a_original.clone();
    let mut b = b_original.clone();

    a.0 = normalize_delgroupall(a.0);
    b.0 = normalize_delgroupall(b.0);
    println!(" a_del*  {:?}", a.0);
    println!(" b_del*  {:?}", b.0);


    let (a_del_0, b_del_0) = transform_deletions(&a.0, &b.0);
    println!(" == a_del_0 {:?}", a_del_0);
    println!(" == b_del_0 {:?}", b_del_0);
    println!();

    // How do you apply del' if add has already been applied on the client?
    // The result will be applied after the client's insert operations had already been performed.
    // Reverse the impact of insA with delA` to not affect already newly added elements or text.
    // var _ = delAfterIns(insA, delA_0, schema), delA_1 = _[0];
    // var _ = delAfterIns(insB, delB_0), delB_1 = _[0];
    println!(" # transform[2] transform_add_del");
    println!(" a_ins   {:?}", a.1);
    println!(" a_del_0 {:?}", a_del_0);
    println!(" ~ transform_add_del()");
    let (a_del_1, a_ins_1) = transform_add_del(&a.1, &a_del_0);
    println!(" == a_del_1 {:?}", a_del_1);
    println!(" == a_ins_1 {:?}", a_ins_1);
    println!();

    println!(" # transform[3] transform_add_del");
    println!(" b_ins   {:?}", b.1);
    println!(" b_del_0 {:?}", b_del_0);
    println!(" ~ transform_add_del()");
    let (b_del_1, b_ins_1) = transform_add_del(&b.1, &b_del_0);
    println!(" == b_del_1 {:?}", b_del_1);
    println!(" == b_ins_1 {:?}", b_ins_1);
    println!();

    // Insertions from both clients must be composed as though they happened against delA` and delB`
    // so that we don't have phantom elements.
    //var _ = oatie._composer(insA, true, delA_1, false).compose().toJSON(), insA1 = _[1];
    // var _ = oatie._composer(insB, true, delB_1, false).compose().toJSON(), insB1 = _[1];
    // let a_ins_1 = a.clone().1;
    // let b_ins_1 = b.clone().1;

    // Transform insert operations together.
    println!(" # transform[4] transform_insertions");
    println!(" a_ins_1 {:?}", a_ins_1);
    println!(" b_ins_1 {:?}", b_ins_1);
    let ((a_del_2, a_ins_2), (b_del_2, b_ins_2)) = transform_insertions(&a_ins_1, &b_ins_1);
    println!(" == a_del_2 {:?}", a_del_2);
    println!(" == a_ins_2 {:?}", a_ins_2); // == a_ins_2 [AddWithGroup([AddWithGroup([AddWithGroup([AddSkip(8)]), AddChars("a")])])]
    println!(" == b_del_2 {:?}", b_del_2);
    println!(" == b_ins_2 {:?}", b_ins_2);
    println!();

    // Our delete operations are now subsequent operations, and so can be composed.
    //var _ = oatie._composer(delA_1, false, delA_2, false).compose().toJSON(), delA_3 = _[0], _ = _[1];
    //var _ = oatie._composer(delB_1, false, delB_2, false).compose().toJSON(), delB_3 = _[0], _ = _[1];
    println!(" # transform[5] compose_del_del");
    println!(" a_del_1 {:?}", a_del_1);
    println!(" a_del_2 {:?}", a_del_2);
    let a_del_3 = compose::compose_del_del(&a_del_1, &a_del_2);
    println!(" == a_del_3 {:?}", a_del_3);
    println!();
    println!(" # transform[6] compose_del_del");
    println!(" b_del_1 {:?}", b_del_1);
    println!(" b_del_2 {:?}", b_del_2);
    let b_del_3 = compose::compose_del_del(&b_del_1, &b_del_2);
    println!(" == b_del_3 {:?}", b_del_3);
    println!();

    println!(" # transform[result]");
    println!(" a_del   {:?}", a.0);
    println!(" a_ins   {:?}", a.1);
    println!(" ~ transform()");
    println!(" =a_del_3  {:?}", a_del_3);
    println!(" =a_ins_2  {:?}", a_ins_2);
    println!(" ---");
    println!(" b_del   {:?}", b.0);
    println!(" b_ins   {:?}", b.1);
    println!(" ~ transform()");
    println!(" =b_del_3  {:?}", b_del_3); // wrong
    println!(" =b_ins_2  {:?}", b_ins_2);
    println!();

    ((a_del_3, a_ins_2), (b_del_3, b_ins_2))
}

// transform with corrections
// TODO

/*
pub fn transform_validate(spanvec: &DocSpan, delvec: &AddSpan) -> (DocSpan, DocSpan) {
    let mut span = &spanvec[..];

    let mut first = None;
    if span.len() > 0 {
        first = Some(span[0].clone());
        span = &span[1..]
    }

    let mut exhausted = first.is_none();

    trace!("ABOUT TO APPLY ADD {:?} {:?}", first, span);

    loop {
        // Flags for whether we have partially or fully consumed an atom.
        let mut nextdel = true;
        let mut nextfirst = true;

        trace!("next {:?} {:?} {:?}", d, first, exhausted);

        match d.clone() {
            AddSkip(count) => match first.clone().unwrap() {
                DocChars(ref value) => {
                    let len = value.chars().count();
                    if len < count {
                        place_chars(&mut res, value.to_owned());
                        d = AddSkip(count - len);
                        nextdel = false;
                    } else if len > count {
                        place_chars(&mut res, value[0..count].to_owned());
                        first = Some(DocChars(value[count..len].to_owned()));
                        nextfirst = false;
                    } else {
                        place_chars(&mut res, value.to_owned());
                    }
                }
                DocGroup(..) => {
                    res.push(first.clone().unwrap());
                    if count > 1 {
                        d = AddSkip(count - 1);
                        nextdel = false;
                    }
                }
            },
            AddWithGroup(ref delspan) => match first.clone().unwrap() {
                DocGroup(ref attrs, ref span) => {
                    res.push(DocGroup(attrs.clone(), apply_add(span, delspan)));
                }
                _ => {
                    panic!("Invalid AddWithGroup");
                }
            },
            AddChars(value) => {
                place_chars(&mut res, value);
                nextfirst = false;
            }
            AddGroup(attrs, innerspan) => {
                let mut subdoc = vec![];
                if !exhausted {
                    subdoc.push(first.clone().unwrap());
                    subdoc.extend_from_slice(span);
                }
                trace!("CALLING INNER {:?} {:?}", subdoc, innerspan);

                let (inner, rest) = apply_add_inner(&subdoc, &innerspan);
                place_any(&mut res, &DocGroup(attrs, inner));

                trace!("REST OF INNER {:?} {:?}", rest, del);

                let inner = apply_add(&rest, &del.to_vec());
                place_many(&mut res, &inner);
                return (res, vec![]);
            }
        }

        if nextdel {
            if del.len() == 0 {
                let mut remaining = vec![];
                if !nextfirst && !first.is_none() && !exhausted {
                    remaining.push(first.clone().unwrap());
                    // place_any(&mut res, &first.clone().unwrap());
                }
                remaining.extend_from_slice(span);
                return (res, remaining);
            }

            d = del[0].clone();
            del = &del[1..];
        }

        if nextfirst {
            if span.len() == 0 {
                exhausted = true;
            } else {
                first = Some(span[0].clone());
                span = &span[1..];
            }
        }
    }
}
*/
