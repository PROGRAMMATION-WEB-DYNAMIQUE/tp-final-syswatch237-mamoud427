// src/main.rs
use chrono::Local;
use std::fmt;
use sysinfo::{System};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
// use std::fs::OpenOptions;

const AUTH_TOKEN: &str = "ENSPD2026";

// --- Types métier ---

#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// --- Affichage ---

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {:.1}% ({} cœurs)", self.usage_percent, self.core_count)
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEM: {}MB utilisés / {}MB total ({} MB libres)",
            self.used_mb, self.total_mb, self.free_mb
        )
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  [{:>6}] {:<25} CPU:{:>5.1}%  MEM:{:>5}MB",
            self.pid, self.name, self.cpu_usage, self.memory_mb
        )
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SysWatch — {} ===", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "--- Top Processus ---")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        write!(f, "=====================")
    }
}

// --- Collecte système ---

fn collect_system_snapshot() -> SystemSnapshot {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_info = CpuInfo {
        usage_percent: sys.global_cpu_info().cpu_usage(),
        core_count: sys.cpus().len(),
    };

    let mem_info = MemInfo {
        total_mb: sys.total_memory() / 1024,
        used_mb: sys.used_memory() / 1024,
        free_mb: sys.free_memory() / 1024,
    };

    let mut processes: Vec<ProcessInfo> = sys.processes()
        .values()
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_mb: p.memory() / 1024,
        })
        .collect();

    // Tri decroissant CPU

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes.truncate(5); // Garder les 5 processus les plus gourmands

    SystemSnapshot {
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        cpu: cpu_info,
        memory: mem_info,
        top_processes: processes,
    }
}

fn format_response(snapshot: &SystemSnapshot, command: &str) -> String {
    match command.trim() {
        // CPU uniquement
        "cpu" => format!("{}", snapshot.cpu),

        // Mémoire uniquement
        "mem" => format!("{}", snapshot.memory),

        // Processus uniquement
        "ps" => {
            if snapshot.top_processes.is_empty() {
                return "Aucun processus disponible".to_string();
            }

            snapshot
                .top_processes
                .iter()
                .map(|p| format!("{}", p))
                .collect::<Vec<String>>()
                .join("\n")
        }

        // Tout afficher
        "all" => format!("{}", snapshot),

        // Aide
        "help" => String::from(
            "Commandes disponibles :
cpu  -> utilisation CPU
mem  -> mémoire
ps   -> top processus
all  -> tout afficher
help -> aide
quit -> quitter",
        ),

        // Quitter
        "quit" => String::from("Connexion fermée"),

        // Commande inconnue
        _ => String::from("Commande inconnue. Tape 'help'"),
    }
}

fn handle_client(stream: TcpStream, data: Arc<Mutex<SystemSnapshot>>) {
    let mut stream = stream;
    let reader = BufReader::new(stream.try_clone().unwrap());

    println!("Client connecté !");

    for line in reader.lines() {
        let command = match line {
            Ok(cmd) => {
                println!("Commande reçue: {}", cmd);
                cmd
            }
            Err(_) => break,
        };

        let snapshot = data.lock().unwrap().clone(); // ✅ important
        let response = format_response(&snapshot, &command);

        println!("Réponse envoyée: {}", response);

        if stream.write_all(response.as_bytes()).is_err() {
            break;
        }

        if stream.write_all(b"\n").is_err() {
            break;
        }

        if stream.flush().is_err() {
            break;
        }

        if command.trim() == "quit" {
            println!("Client déconnecté.");
            break;
        }
    }
}

// Main Exo 1: Types métier et affichage — Etape 3: Affichage humain avec le trait Display
fn main() {
    // Snapshot partagé
    let snapshot = Arc::new(Mutex::new(collect_system_snapshot()));

    // Thread de mise à jour (toutes les 5 secondes)
    {
        let data = Arc::clone(&snapshot);
        thread::spawn(move || loop {
            let mut locked = data.lock().unwrap();
            *locked = collect_system_snapshot();
            thread::sleep(Duration::from_secs(5));
        });
    }

    // Serveur TCP
    let listener = TcpListener::bind("0.0.0.0:7878").unwrap();
    println!("Serveur lancé sur le port 7878 !");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let data = Arc::clone(&snapshot);

                thread::spawn(move || {
                    handle_client(stream, data);
                });
            }
            Err(e) => {
                println!("Erreur connexion: {}", e);
            }
        }
    }
}