use maze_wars::protocol::{
    ClientMessage, MAX_PACKET_SIZE, MAX_USERNAME_BYTES, ServerMessage, decode_server, encode_client,
};
use std::io::{self, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:34254";
const CONNECTION_ATTEMPTS: usize = 3;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);

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
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|error| format!("could not open a UDP socket: {error}"))?;
    socket
        .connect(server)
        .map_err(|error| format!("could not use server address {server}: {error}"))?;
    socket
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(|error| format!("could not configure the UDP socket: {error}"))?;

    println!("Using UDP connection to {server}");
    let username = ask_for_username()?;
    println!("Starting...");

    let (player_id, level) = join_server(&socket, server, &username)?;
    println!("Connected as player {player_id} on level {level}");
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

fn join_server(
    socket: &UdpSocket,
    server: SocketAddr,
    username: &str,
) -> Result<(u32, u8), String> {
    let join_packet = encode_client(&ClientMessage::Join {
        username: username.to_owned(),
    })
    .map_err(|error| format!("could not prepare the join request: {error}"))?;
    let mut buffer = [0_u8; MAX_PACKET_SIZE];

    for attempt in 1..=CONNECTION_ATTEMPTS {
        match socket.send(&join_packet) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {}
            Err(error) => return Err(format!("could not send data to {server}: {error}")),
        }

        match socket.recv(&mut buffer) {
            Ok(length) => {
                let response = decode_server(&buffer[..length])
                    .map_err(|error| format!("server sent an invalid response: {error}"))?;
                return match response {
                    ServerMessage::Accepted { player_id, level } => Ok((player_id, level)),
                    ServerMessage::Rejected { reason } => {
                        Err(format!("server rejected the connection: {reason}"))
                    }
                };
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionRefused
                ) =>
            {
                if attempt < CONNECTION_ATTEMPTS {
                    println!(
                        "No response from server, retrying ({attempt}/{CONNECTION_ATTEMPTS})..."
                    );
                }
            }
            Err(error) => return Err(format!("could not receive data from {server}: {error}")),
        }
    }

    Err(format!(
        "server {server} is not running or reachable; start it first with `cargo run --bin server`"
    ))
}
