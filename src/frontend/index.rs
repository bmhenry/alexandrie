use bigdecimal::{BigDecimal, ToPrimitive};
use diesel::dsl as sql;
use diesel::prelude::*;
use json::json;
use tide::{Request, Response};

use crate::db::schema::*;
use crate::db::DATETIME_FORMAT;
use crate::error::Error;
use crate::frontend::helpers;
use crate::index::Indexer;
use crate::utils;
use crate::utils::auth::AuthExt;
use crate::State;

pub(crate) async fn get(req: Request<State>) -> Result<Response, Error> {
    let user = req.get_author();
    let state = req.state().clone();
    let repo = &state.repo;

    let transaction = repo.transaction(move |conn| {
        let state = req.state();

        //? Get total number of crates.
        let crate_count = crates::table
            .select(sql::count(crates::id))
            .first::<i64>(conn)?;

        //? Get total number of crate downloads.
        let total_downloads = crates::table
            .select(sql::sum(crates::downloads))
            .first::<Option<BigDecimal>>(conn)?
            .map_or(0, |dec| {
                dec.to_u64()
                    .expect("download count exceeding u64::max_value()")
            });

        //? Get the 10 most downloaded crates.
        let most_downloaded = crates::table
            .select((crates::name, crates::description, crates::downloads))
            .order_by(crates::downloads.desc())
            .limit(10)
            .load::<(String, Option<String>, i64)>(conn)?;

        //? Get the 10 most recently updated crates.
        let last_updated = crates::table
            .select((crates::name, crates::description, crates::updated_at))
            .order_by(crates::updated_at.desc())
            .limit(10)
            .load::<(String, Option<String>, String)>(conn)?;

        let engine = &state.frontend.handlebars;
        let context = json!({
            "user": user,
            "instance": &state.frontend.config,
            "total_downloads": helpers::humanize_number(total_downloads),
            "crate_count": helpers::humanize_number(crate_count),
            "index_url": state.index.url().as_mut().map(|url| url.trim()).ok(),
            "most_downloaded": most_downloaded.into_iter().map(|(name, description, downloads)| json!({
                "name": name,
                "version": state.index.latest_record(&name).map(|rec| rec.vers).ok(),
                "description": description,
                "downloads": helpers::humanize_number(downloads),
            })).collect::<Vec<_>>(),
            "last_updated": last_updated.into_iter().map(|(name, description, date)| {
                let updated_at = chrono::NaiveDate::parse_from_str(date.as_str(), DATETIME_FORMAT).unwrap();
                json!({
                    "name": name,
                    "description": description,
                    "updated_at": helpers::humanize_date(updated_at),
                })
            }).collect::<Vec<_>>(),
        });
        Ok(utils::response::html(
            engine.render("index", &context).unwrap(),
        ))
    });

    transaction.await
}
