//! `mau seed --user NAME`: fills a development account with the sample books
//! from the design mocks. Refuses to run unless `public_url` is localhost.

use anyhow::{Result, bail};
use mau_core::hlc::Clock;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::library::{self, Person};
use crate::util::{new_id, now_ms};

struct Sample {
    title: &'static str,
    authors: &'static [&'static str],
    year: i64,
    pages: i64,
    tags: &'static [&'static str],
    owned: Option<(&'static str, &'static str)>,
    state: State,
    rating: i64,
}

enum State {
    /// Active, at page `pos`, started `days` ago.
    Active {
        pos: i64,
        days: i64,
    },
    /// Inactive with progress.
    Paused {
        pos: i64,
        days: i64,
    },
    Unstarted,
    /// Finished `ago` days ago after reading for `took` days. Optionally an earlier reading.
    Finished {
        ago: i64,
        took: i64,
        earlier: Option<(i64, i64)>,
    },
}

const SAMPLES: &[Sample] = &[
    Sample {
        title: "The Left Hand of Darkness",
        authors: &["Ursula K. Le Guin"],
        year: 1969,
        pages: 304,
        tags: &["sci-fi", "classics"],
        owned: Some(("Paperback", "Living room, shelf 3")),
        state: State::Active { pos: 140, days: 27 },
        rating: 0,
    },
    Sample {
        title: "Middlemarch",
        authors: &["George Eliot"],
        year: 1871,
        pages: 880,
        tags: &["classics"],
        owned: Some(("Paperback", "Bedroom")),
        state: State::Active { pos: 299, days: 60 },
        rating: 0,
    },
    Sample {
        title: "A Wizard of Earthsea",
        authors: &["Ursula K. Le Guin"],
        year: 1968,
        pages: 183,
        tags: &["fantasy"],
        owned: Some(("Paperback", "Living room, shelf 3")),
        state: State::Active { pos: 182, days: 20 },
        rating: 0,
    },
    Sample {
        title: "Gödel, Escher, Bach",
        authors: &["Douglas Hofstadter"],
        year: 1979,
        pages: 777,
        tags: &["non-fiction"],
        owned: Some(("Hardcover", "Office")),
        state: State::Active { pos: 212, days: 45 },
        rating: 0,
    },
    Sample {
        title: "Station Eleven",
        authors: &["Emily St. John Mandel"],
        year: 2014,
        pages: 333,
        tags: &["sci-fi", "book club"],
        owned: None,
        state: State::Unstarted,
        rating: 0,
    },
    Sample {
        title: "The Brothers Karamazov",
        authors: &["Fyodor Dostoevsky"],
        year: 1880,
        pages: 796,
        tags: &["classics"],
        owned: Some(("Paperback", "Bedroom")),
        state: State::Paused {
            pos: 310,
            days: 120,
        },
        rating: 0,
    },
    Sample {
        title: "Thinking, Fast and Slow",
        authors: &["Daniel Kahneman"],
        year: 2011,
        pages: 499,
        tags: &["non-fiction"],
        owned: None,
        state: State::Paused {
            pos: 160,
            days: 200,
        },
        rating: 0,
    },
    Sample {
        title: "Pale Fire",
        authors: &["Vladimir Nabokov"],
        year: 1962,
        pages: 315,
        tags: &["classics"],
        owned: Some(("Paperback", "Office")),
        state: State::Unstarted,
        rating: 0,
    },
    Sample {
        title: "The Name of the Rose",
        authors: &["Umberto Eco"],
        year: 1980,
        pages: 536,
        tags: &["classics"],
        owned: Some(("Hardcover", "Living room")),
        state: State::Paused { pos: 45, days: 300 },
        rating: 0,
    },
    Sample {
        title: "Hyperion",
        authors: &["Dan Simmons"],
        year: 1989,
        pages: 482,
        tags: &["sci-fi"],
        owned: Some(("Paperback", "Office")),
        state: State::Unstarted,
        rating: 0,
    },
    Sample {
        title: "Klara and the Sun",
        authors: &["Kazuo Ishiguro"],
        year: 2021,
        pages: 303,
        tags: &["book club"],
        owned: None,
        state: State::Unstarted,
        rating: 0,
    },
    Sample {
        title: "Piranesi",
        authors: &["Susanna Clarke"],
        year: 2020,
        pages: 245,
        tags: &["fantasy"],
        owned: Some(("Hardcover", "Bedroom")),
        state: State::Finished {
            ago: 12,
            took: 20,
            earlier: None,
        },
        rating: 4,
    },
    Sample {
        title: "The Remains of the Day",
        authors: &["Kazuo Ishiguro"],
        year: 1989,
        pages: 258,
        tags: &["book club"],
        owned: Some(("Paperback", "Living room")),
        state: State::Finished {
            ago: 59,
            took: 25,
            earlier: None,
        },
        rating: 5,
    },
    Sample {
        title: "Project Hail Mary",
        authors: &["Andy Weir"],
        year: 2021,
        pages: 476,
        tags: &["sci-fi"],
        owned: None,
        state: State::Finished {
            ago: 109,
            took: 15,
            earlier: None,
        },
        rating: 3,
    },
    Sample {
        title: "Braiding Sweetgrass",
        authors: &["Robin Wall Kimmerer"],
        year: 2013,
        pages: 391,
        tags: &["non-fiction"],
        owned: Some(("Paperback", "Office")),
        state: State::Finished {
            ago: 183,
            took: 40,
            earlier: None,
        },
        rating: 5,
    },
    Sample {
        title: "The Dispossessed",
        authors: &["Ursula K. Le Guin"],
        year: 1974,
        pages: 387,
        tags: &["sci-fi", "classics"],
        owned: Some(("Paperback", "Bedroom")),
        state: State::Finished {
            ago: 235,
            took: 37,
            earlier: Some((2700, 45)),
        },
        rating: 5,
    },
];

pub async fn run(config: &Config, db: &SqlitePool, username: &str) -> Result<()> {
    if !config.public_url.starts_with("http://localhost") {
        bail!(
            "`mau seed` only runs on development instances (public_url must be http://localhost…)"
        );
    }
    let user: Option<(String, String)> =
        sqlx::query_as("SELECT id, time_zone FROM users WHERE username = ?")
            .bind(username.to_ascii_lowercase())
            .fetch_optional(db)
            .await?;
    let Some((user_id, _tz)) = user else {
        bail!("no user named {username:?}; sign up first")
    };

    let mut clock = Clock::new("seed");
    let mut tx = db.begin().await?;
    let seq = library::next_seq(&mut tx).await?;
    let today = crate::util::today_days();
    let ranks = mau_core::rank::spread(SAMPLES.len());
    let mut rng_state: u64 = 0x2545F4914F6CDD1D;
    let mut rand = move |n: i64| -> i64 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        (rng_state % n.max(1) as u64) as i64
    };

    for (i, s) in SAMPLES.iter().enumerate() {
        let work_id = new_id();
        sqlx::query("INSERT INTO works (id, title, first_year, source, created_at, seq) VALUES (?, ?, ?, 'seed', ?, ?)")
            .bind(&work_id)
            .bind(s.title)
            .bind(s.year)
            .bind(now_ms())
            .bind(seq)
            .execute(&mut *tx)
            .await?;
        let people: Vec<Person> = s
            .authors
            .iter()
            .map(|a| Person {
                name: a.to_string(),
                role: "author".into(),
            })
            .collect();
        library::insert_people(&mut tx, &work_id, &people).await?;
        let edition_id = new_id();
        sqlx::query("INSERT INTO editions (id, work_id, binding, year, pages, source, seq) VALUES (?, ?, 'Paperback', ?, ?, 'seed', ?)")
            .bind(&edition_id)
            .bind(&work_id)
            .bind(s.year)
            .bind(s.pages)
            .bind(seq)
            .execute(&mut *tx)
            .await?;

        let ts = |c: &mut Clock, days_ago: i64| {
            c.now(((today - days_ago) * 86_400_000) as u64 + 43_200_000)
                .to_string()
        };
        let book_id = new_id();
        let active = matches!(s.state, State::Active { .. });
        let added_days = match s.state {
            State::Active { days, .. } | State::Paused { days, .. } => days + 5,
            State::Unstarted => 10 + i as i64 * 3,
            State::Finished { ago, took, earlier } => {
                earlier.map_or(ago + took + 3, |(a, t)| a + t + 3)
            }
        };
        let h = ts(&mut clock, added_days);
        let book_clock = format!(
            r#"{{"rank":"{h}","active":"{h}","rating":"{h}","notes":"{h}","title_override":"{h}","authors_override":"{h}","cover_override":"{h}"}}"#
        );
        sqlx::query(
            "INSERT INTO books (id, user_id, work_id, rank, active, rating, added_at, clock, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&book_id)
        .bind(&user_id)
        .bind(&work_id)
        .bind(&ranks[i])
        .bind(active)
        .bind(s.rating)
        .bind((today - added_days) * 86_400_000)
        .bind(&book_clock)
        .bind(seq)
        .execute(&mut *tx)
        .await?;
        for tag in s.tags {
            sqlx::query("INSERT INTO book_tags (book_id, user_id, tag, present, clock, seq) VALUES (?, ?, ?, 1, ?, ?)")
                .bind(&book_id)
                .bind(&user_id)
                .bind(tag)
                .bind(&h)
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }
        if let Some((format, location)) = s.owned {
            sqlx::query("INSERT INTO copies (id, book_id, user_id, format, location, edition_id, clock, seq) VALUES (?, ?, ?, ?, ?, ?, '{}', ?)")
                .bind(new_id())
                .bind(&book_id)
                .bind(&user_id)
                .bind(format)
                .bind(location)
                .bind(&edition_id)
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }

        // Readings: (started days ago, last day read days ago, end position, finished?)
        let readings: Vec<(i64, i64, i64, bool)> = match s.state {
            State::Active { pos, days } => vec![(days, 0, pos, false)],
            State::Paused { pos, days } => vec![(days, days - 40, pos, false)],
            State::Unstarted => vec![],
            State::Finished { ago, took, earlier } => {
                let mut v = vec![];
                if let Some((a, t)) = earlier {
                    v.push((a + t, a, s.pages, true));
                }
                v.push((ago + took, ago, s.pages, true));
                v
            }
        };
        for (start, end, target, finished) in readings {
            let reading_id = new_id();
            let created = ts(&mut clock, start);
            sqlx::query(
                "INSERT INTO readings (id, book_id, user_id, started_on, total, pos_clock, clock, seq) VALUES (?, ?, ?, ?, ?, ?, '{}', ?)",
            )
            .bind(&reading_id)
            .bind(&book_id)
            .bind(&user_id)
            .bind(crate::util::date_string(today - start))
            .bind(s.pages)
            .bind(&created)
            .bind(seq)
            .execute(&mut *tx)
            .await?;
            let mut pos = 0;
            let mut pos_clock = created.clone();
            let span = (start - end).max(1);
            for d in (end..=start).rev() {
                if pos >= target {
                    break;
                }
                let last_day = d == end;
                if !last_day && rand(10) >= 6 {
                    continue;
                }
                let remaining_days = (d - end + 1).max(1);
                let step = if last_day {
                    target - pos
                } else {
                    ((target - pos) / remaining_days * (5 + rand(15)) / 10).clamp(1, target - pos)
                };
                let hlc = ts(&mut clock, d);
                sqlx::query("INSERT INTO progress_events (id, reading_id, user_id, hlc, local_date, from_page, to_page, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(new_id())
                    .bind(&reading_id)
                    .bind(&user_id)
                    .bind(&hlc)
                    .bind(crate::util::date_string(today - d))
                    .bind(pos)
                    .bind(pos + step)
                    .bind(seq)
                    .execute(&mut *tx)
                    .await?;
                pos += step;
                pos_clock = hlc;
            }
            let _ = span;
            sqlx::query("UPDATE readings SET finished_on = ?, pos = ?, pos_clock = ?, clock = ? WHERE id = ?")
                .bind(finished.then(|| crate::util::date_string(today - end)))
                .bind(pos)
                .bind(&pos_clock)
                .bind(format!(r#"{{"total":"{created}","finished_on":"{pos_clock}"}}"#))
                .bind(&reading_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    println!("Added {} sample books to @{username}.", SAMPLES.len());
    Ok(())
}
