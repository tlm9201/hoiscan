// clap
use clap::Parser;

use hoicolor::Converter;
// steamworks
use steamworks::Client;
use steamworks::DistanceFilter;
use steamworks::LobbyId;
use steamworks::LobbyKey;
use steamworks::Matchmaking;
use steamworks::ClientManager;
use steamworks::NearFilter;
use steamworks::NumberFilter;
use steamworks::SingleClient;
use steamworks::StringFilter;
use steamworks::StringFilterKind;
use steamworks::LobbyListFilter;

use core::panic;
// std
use std::time::Duration;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread::sleep;

// tables
#[macro_use] extern crate prettytable;
use prettytable::Table;
use prettytable::format;
use prettytable::format::TableFormat;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {

    // the name of the game to filter
    #[arg(short, long, default_value = "")]
    name: String,

    // should we only include games with no password 
    #[clap(long, short='p', action)]
    no_password: bool,

    // Should we only include vanilla games 
    #[clap(long, short, action)]
    vanilla_only: bool,

    // The search refresh interval in seconds, if not set, only search once
    #[clap(short, long, default_value_t = 0)]
    interval: u64
}

struct Game {
    name: String,
    version: String,
    password: bool,
    max_players: usize,
    current_players: usize,
    id: u64,
}

const VANILLA_CHECKSUM: &str = "0143";

fn main() {
    let args = Args::parse();
    let client: (Client, SingleClient) = Client::init_app(394360).unwrap();

    let mut matchmaking = client.0.matchmaking();
    let (sender_lobbies, reciever_lobbies) = mpsc::channel();

    // setup callback thread
    let callback_thread = std::thread::spawn(move || {
        loop {
            // run callbacks
            client.1.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    matchmaking = request_lobbies(&args, matchmaking, sender_lobbies);
    run(&args, &reciever_lobbies, matchmaking);
}

fn run(args: &Args, reciever_lobbies: &Receiver<Vec<LobbyId>>, matchmaking: Matchmaking<ClientManager>) {
    // find and show games once
    if args.interval <= 0 {
        let results = find_games(&reciever_lobbies, &matchmaking);
        show_results(&results);
        return;
    }

    // otherwise, loop on interval, compare old -> games, and show each interval

    // store an instance of the last games to compare
    let mut last_games: Vec<Game> = Vec::new();
    loop {
        let found_games: Vec<Game> = find_games(&reciever_lobbies, &matchmaking);
        let mut games: Vec<Game> = Vec::new();

        if last_games.len() != 0 {
            for new_game in found_games {
                for old_game in &last_games {
                    if !old_game.name.eq(&new_game.name) {
                        games.push(new_game);
                        break;
                    }
                }
            }
        } else {
            games = found_games;
        }
        
        // show the filtered games
        if games.len() > 0 {
            show_results(&games);
            last_games = games;
        }

        // update last games
        // sleep on secs interval
        sleep(Duration::from_secs(args.interval));
    }
}

fn find_games(reciever_lobbies: &Receiver<Vec<LobbyId>>, matchmaking: &Matchmaking<ClientManager>) -> Vec<Game> {
    let mut result = Vec::new();
    if let Ok(lobbies) = reciever_lobbies.recv_timeout(Duration::from_secs(10)) {
        for lobby_id in lobbies {
            result.push(game(lobby_id, &matchmaking));
        }
    } else {
        println!("Request timed out.");
    };

    return result;
}

fn request_lobbies(args: &Args, matchmaking: Matchmaking<ClientManager>, sender_lobbies: Sender<Vec<LobbyId>>) -> Matchmaking<ClientManager>{
    let mut sf: Vec<StringFilter> = Vec::new();

    if !args.name.is_empty() {
        sf.push(StringFilter(LobbyKey::new("name"), &args.name, StringFilterKind::Include));
    }
    
    if args.no_password {
        sf.push(StringFilter(LobbyKey::new("password"), "0", StringFilterKind::Include));
    } 

    if args.vanilla_only {
        sf.push(StringFilter(LobbyKey::new("version"), VANILLA_CHECKSUM, StringFilterKind::Include));
    }

    matchmaking.set_lobby_list_filter(
        LobbyListFilter {
            string: Some(vec![StringFilter(LobbyKey::new("name"), "hiiiiiiiiiiiiiiiiiiiiiiiiii", StringFilterKind::Include)]),
            //distance: Some(DistanceFilter::Close),
            ..Default::default()
        }
    ).request_lobby_list(move |_cb| match _cb {
        Ok(lobbies) => {
            println!("{:?}", lobbies);
            sender_lobbies.send(lobbies).unwrap();
        }

        Err(err) => panic!("Error: {}", err)
    });


    return matchmaking;
}

fn game(lobby_id: LobbyId, matchmaking: &Matchmaking<ClientManager>) -> Game {
    let game = Game {
        name: matchmaking.lobby_data(lobby_id, "name").unwrap_or("").to_owned(),
        version: matchmaking.lobby_data(lobby_id, "version").unwrap_or("").to_owned(),
        password: if matchmaking.lobby_data(lobby_id, "password").unwrap_or("").to_owned() == "1" {true} else {false},
        max_players: matchmaking.lobby_member_limit(lobby_id).unwrap_or(64).to_owned(),
        current_players: matchmaking.lobby_member_count(lobby_id),
        id: lobby_id.raw()
    };

    return game;
}

fn show_results(games: &Vec<Game>) {
    // create the table
    let mut table = Table::new();
    let format: TableFormat = format::FormatBuilder::new()
        .column_separator('|')
        .borders('|')
        .separators(&[format::LinePosition::Top,
            format::LinePosition::Bottom],
            format::LineSeparator::new('-', '+', '+', '+'))
        .padding(1, 1)
        .build();
    table.set_format(format);

    // header
    table.set_titles(row!["name", "version", "password", "players", "id"]);

    // fill
    for game in games {
        let players = game.current_players.to_string() + "/" + &game.max_players.to_string();
        table.add_row(row![trim_name(&game.name), game.version, game.password, players, game.id]);
    }

    // print the table to stdout
    table.printstd();
}

// trims game name to a char limit
// also parses hoi color
fn trim_name(name: &str) -> String {
    let converter: Converter = Converter(name.to_owned());
    let parsed = converter.to_ansi();
    if name.len() > 50 {
        let trimmed = parsed[0..47].to_owned();
        return trimmed + "...";
    }

    return parsed;
}
