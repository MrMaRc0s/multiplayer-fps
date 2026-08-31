use maze_wars::client::{DEFAULT_SERVER_ADDRESS, connect};
use maze_wars::protocol::MAX_USERNAME_BYTES;
use std::io::{self, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Could not start Maze Wars: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let server = ask_for_server()?;
    println!("Using UDP connection to {server}");
    let username = ask_for_username()?;
    println!("Starting...");

    let connection = connect(server, &username)?;
    println!(
        "Connected as player {} on level {}",
        connection.player_id(),
        connection.level()
    );
    Ok(())
}

fn ask_for_server() -> Result<SocketAddr, String> {
    let input = prompt(&format!("Enter IP Address [{DEFAULT_SERVER_ADDRESS}]: "))?;
    let address = if input.is_empty() {
        DEFAULT_SERVER_ADDRESS
    } else {
        input.as_str()
    };

    address
        .to_socket_addrs()
        .map_err(|error| format!("invalid server address '{address}': {error}"))?
        .next()
        .ok_or_else(|| format!("server address '{address}' did not resolve"))
}

fn ask_for_username() -> Result<String, String> {
    loop {
        let username = prompt("Enter Name: ")?;
        if !username.is_empty() && username.len() <= MAX_USERNAME_BYTES {
            return Ok(username);
        }

        println!("Name must contain 1 to {MAX_USERNAME_BYTES} bytes.");
    }
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not write to the terminal: {error}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("could not read from the terminal: {error}"))?;
    Ok(input.trim().to_owned())
}
