extern crate dotenv;

extern crate lib_flashcard;
extern crate webe_auth;
extern crate webe_id;
extern crate webe_web;

use std::env;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

use lib_flashcard::http::*;
use webe_auth::http::{create_account, login, logout, secure, verify_account};
use webe_web::responders::{file::FileResponder, options::OptionsResponder, spa::SPAResponder};
use webe_web::server::{Route, RouteMap, Server};

#[tokio::main]
async fn main() {
    // load environment
    print!("Loading Environment Config......");
    dotenv::dotenv().expect("Failed to load environment config file");
    println!("Done");

    // create the email pool
    print!("Building Email Connection pool......");
    let smtp_address = env::var("SMTP_ADDRESS").expect("Failed to load SMTP Address from .env");
    let smtp_user = env::var("SMTP_USER").expect("Failed to load SMTP User from .env");
    let smtp_pass = env::var("SMTP_PASS").expect("Failed to load SMTP Password from .env");
    let email_pool = webe_auth::email::create_smtp_pool(smtp_address, smtp_user, smtp_pass)
        .expect("Failed to create SMTP pool");
    println!("Done");

    // create the Auth database pool
    print!("Building AUTH Database Connection Pool......");
    let db_connect_string =
        env::var("AUTH_DATABASE_URL").expect("Failed to load Auth DB Connect string from .env");
    let auth_db_manager = webe_auth::db::new_manager(db_connect_string)
        .expect("Failed to create Auth Database connection pool");
    println!("Done");

    // create the unique ID factory
    let node_id = 0u8;
    let epoch = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_millis(1546300800000)) // 01-01-2019 12:00:00 AM GMT
        .expect("failed to create custom epoch");
    let id_factory = Arc::new(std::sync::Mutex::new(
        webe_id::WebeIDFactory::new(epoch, node_id).expect("Failed to create ID generator"),
    ));

    // create the auth manager
    let auth_manager = Arc::new(Mutex::new(webe_auth::WebeAuth {
        db_manager: auth_db_manager,
        email_manager: email_pool,
        id_factory: id_factory.clone(),
    }));

    // create the Flash database pool
    print!("Building FLASH Database Connection Pool......");
    let db_connect_string =
        env::var("FLASH_DATABASE_URL").expect("Failed to load Flash DB Connect string from .env");
    let flash_db_manager = webe_auth::db::new_manager(db_connect_string)
        .expect("Failed to create Flash Database connection pool");
    println!("Done");

    // create the flash manager
    let flash_manager = Arc::new(Mutex::new(lib_flashcard::FlashManager {
        db_manager: flash_db_manager,
        id_factory: id_factory.clone(),
    }));

    // create the web server
    print!("Setting up Web Server and Routes......");
    let web_bind_ip = env::var("WEB_BIND_IP").expect("Failed to load Web Server Bind IP from .env");
    let web_bind_port =
        env::var("WEB_BIND_PORT").expect("Failed to load Web Server Bind PORT from .env");
    let ip = web_bind_ip
        .parse::<Ipv4Addr>()
        .expect("Failed to parse WEB_BIND_IP as Ipv4Addr");
    let port = web_bind_port
        .parse::<u16>()
        .expect("Failed to parse WEB_BIND_PORT as u16");
    let web_server = Server::new(&ip, &port)
        .await
        .expect("Failed to create web server");

    // add routes
    let mut route_map = RouteMap::new();
    // -- OPTIONS for preflight request
    let options_route = Route::new("OPTIONS", "/<dump>");
    let options_responder = OptionsResponder::new(
        "http://localhost:1234".to_owned(),
        "POST, GET, OPTIONS".to_owned(),
        "content-type, x-webe-token".to_owned(),
    );
    route_map.add_route(options_route, options_responder);

    // -- auth
    // -- -- account
    let create_account_route = Route::new("POST", "/account/create");
    let create_account_responder =
        create_account::CreateAccountResponder::new(auth_manager.clone());
    route_map.add_route(create_account_route, create_account_responder);

    let verify_account_route = Route::new("POST", "/account/verify");
    let verify_account_responder =
        verify_account::VerifyAccountResponder::new(auth_manager.clone());
    route_map.add_route(verify_account_route, verify_account_responder);

    // -- -- session
    let login_route = Route::new("POST", "/account/login");
    let login_responder = login::LoginResponder::new(auth_manager.clone());
    route_map.add_route(login_route, login_responder);

    let logout_route = Route::new("POST", "/account/logout");
    let logout_responder = logout::LogoutResponder::new(auth_manager.clone());
    route_map.add_route(logout_route, logout_responder);

    // -- flashcard
    // -- -- deck
    let get_decks_route = Route::new("GET", "/decks");
    let get_decks_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        deck::DecksResponder::new(flash_manager.clone()),
    );
    route_map.add_route(get_decks_route, get_decks_responder);

    let get_deck_details_route = Route::new("GET", "/deck/<id>");
    let get_deck_details_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        deck::DeckDetailsResponder::new(flash_manager.clone(), "<id>".to_string()),
    );
    route_map.add_route(get_deck_details_route, get_deck_details_responder);

    let create_deck_route = Route::new("POST", "/deck/create");
    let create_deck_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        deck::CreateDeckResponder::new(flash_manager.clone()),
    );
    route_map.add_route(create_deck_route, create_deck_responder);

    let rename_deck_route = Route::new("POST", "/deck/rename");
    let rename_deck_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        deck::UpdateDeckResponder::new(flash_manager.clone()),
    );
    route_map.add_route(rename_deck_route, rename_deck_responder);

    let delete_deck_route = Route::new("POST", "/deck/delete");
    let delete_deck_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        deck::DeleteDeckResponder::new(flash_manager.clone()),
    );
    route_map.add_route(delete_deck_route, delete_deck_responder);

    // -- -- card

    let create_card_route = Route::new("POST", "/card/create");
    let create_card_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        card::CreateCardResponder::new(flash_manager.clone()),
    );
    route_map.add_route(create_card_route, create_card_responder);

    let update_card_route = Route::new("POST", "/card/update");
    let update_card_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        card::UpdateCardResponder::new(flash_manager.clone()),
    );
    route_map.add_route(update_card_route, update_card_responder);

    let update_card_position_route = Route::new("POST", "/card/updatepos");
    let update_card_position_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        card::UpdateCardPositionResponder::new(flash_manager.clone()),
    );
    route_map.add_route(update_card_position_route, update_card_position_responder);

    let delete_card_route = Route::new("POST", "/card/delete");
    let delete_card_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        card::DeleteCardResponder::new(flash_manager.clone()),
    );
    route_map.add_route(delete_card_route, delete_card_responder);

    let update_score_route = Route::new("POST", "/card/score");
    let update_score_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        game::UpdateScoreResponder::new(flash_manager.clone()),
    );
    route_map.add_route(update_score_route, update_score_responder);

    let get_deck_scores_route = Route::new("GET", "/deck/scores/<id>");
    let get_deck_scores_responder = secure::SecureResponder::new(
        auth_manager.clone(),
        game::DeckScoresResponder::new(flash_manager.clone(), "<id>".to_string()),
    );
    route_map.add_route(get_deck_scores_route, get_deck_scores_responder);

    // -- app
    let file_route = Route::new("GET", "/app/<path>");
    let file_responder = FileResponder::new("./app".to_owned(), "<path>".to_owned())
        .expect("Failed to create FileResponder");
    route_map.add_route(file_route, file_responder);

    // Anything not above needs to be caught by a wildcard and redirected to index.html
    let app_route = Route::new("GET", "/<wildcard>");
    let app_responder = SPAResponder::new(".".to_owned(), "app/index.html".to_owned())
        .expect("Failed to create SPAResponder");
    route_map.add_route(app_route, app_responder);

    // start the server
    println!("Done");
    println!("___FLASHCARD SERVER IS RUNNING___");
    let _start_result = web_server.start(route_map).await;
}
