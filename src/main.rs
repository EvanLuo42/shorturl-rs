use std::{env, sync::Arc};

use axum::{
    extract::{Path, State}, http::{StatusCode, Uri}, response::Redirect, routing::get, Extension, Json, Router
};

use deadpool_diesel::{sqlite, Runtime};
use diesel::prelude::*;
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use dotenvy::dotenv;
use errors::internal_error;
use nanoid::nanoid;
use schema::urls::{self};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

mod schema;
mod errors;

struct AppConfig {
    addr: String,
    nano_id_alphabet: [char; 16]
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

#[derive(Serialize, Selectable, Queryable)]
struct Url {
    id: String,
    url: String
}

#[derive(Deserialize, Insertable, Clone)]
#[diesel(table_name = urls)]
struct NewUrl {
    id: String,
    url: String
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let db_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let manager = 
        sqlite::Manager::new(db_url, Runtime::Tokio1);
    let pool = sqlite::Pool::builder(manager)
        .build()
        .unwrap();

    let alphabet: [char; 16] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f'
    ];

    let config = AppConfig {
        addr: "127.0.0.1:3000".into(),
        nano_id_alphabet: alphabet
    };

    let app = Router::new()
        .route("/url/add/:origin_url", get(add_url))
        .route("/:id", get(redirect_to))
        .layer(Extension(Arc::new(config)))
        .with_state(pool);

    let listener = TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("ShortURL service has been run on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn add_url(
    Path(origin_url): Path<String>,
    State(pool): State<sqlite::Pool>,
    Extension(config): Extension<Arc<AppConfig>>
) -> Result<Json<AddUrlResponse>, (StatusCode, String)> {
    if origin_url.parse::<Uri>().is_err() {
        return Err(
            (StatusCode::BAD_REQUEST, "Not a valid URL".into())
        )
    }

    let conn = pool.get()
        .await
        .map_err(internal_error)?;

    let new_url = NewUrl {
        id: nanoid!(10, &config.nano_id_alphabet),
        url: origin_url.clone()
    };
    let _new_url = new_url.clone();

    conn.interact(move |conn| {
        let _ = diesel::insert_into(urls::table)
            .values(new_url.clone())
            .execute(conn);
    })
    .await
    .map_err(internal_error)?;

    Ok(
        Json(AddUrlResponse {
            gen_url: format!("{}/{}", config.addr, _new_url.id),
            origin_url
        })
    )
}

async fn redirect_to(
    Path(id): Path<String>,
    State(pool): State<sqlite::Pool>
) -> Result<Redirect, (StatusCode, String)> {
    let conn = pool.get()
        .await
        .map_err(internal_error)?;
    conn.interact(|conn| {
        let _ = urls::table
            .filter(urls::id.eq(id))
            .select(Url::as_select())
            .get_result(conn);
    })
    .await
    .map_err(internal_error)?;

    Ok(Redirect::to(""))
}

#[derive(Serialize)]
struct AddUrlResponse {
    gen_url: String,
    origin_url: String
}