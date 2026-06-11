#[allow(non_snake_case)]
mod Agent;
#[allow(non_snake_case)]
mod Control;
mod DB;
#[allow(non_snake_case)]
mod Net;
mod Neture;
#[allow(non_snake_case)]
mod Tools;
mod config;
#[allow(non_snake_case)]
mod UI;

use config::Config;
use crossterm::style::{style, Stylize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use Tools::Tool;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--demo" {
        run_demo();
    } else {
        run_interactive();
    }
}

/// Interactive CLI
fn run_interactive() {
    UI::print_banner();
    println!();

    // Load config
    let mut config = Config::load_or_default();

    // Show config summary at startup
    println!(
        "{} {}  {} {}\n",
        style("provider:").with(UI::DIM),
        style(&config.llm.provider).with(UI::ACCENT).bold(),
        style("model:").with(UI::DIM),
        style(&config.llm.model).with(UI::ACCENT),
    );

    // Build tool list
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(Tools::LS::LsTool),
        Box::new(Tools::WRITE::WriteTool),
        Box::new(Tools::EDIT::EditTool),
        Box::new(Tools::DEL::DelTool),
        Box::new(Tools::EXEC::ExecTool),
        Box::new(Tools::CHECK::CheckTool),
        Box::new(Tools::PLUGS::PlugsTool),
        Box::new(Tools::Todo::TodoTool),
    ];

    let mut agent: Option<Agent::Agent> = None;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    loop {
        UI::prompt();

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                println!("\nGoodbye.");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
        }

        let input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }

        // Handle slash commands
        if input.starts_with('/') {
            handle_command(&input, &mut config, &mut agent, &tools);
            continue;
        }

        // Ensure Agent exists
        if agent.is_none() {
            agent = Some(Agent::Agent::new(config.clone(), create_tools()));
        }

        // Chat
        let ag = agent.as_mut().unwrap();
        print!("{} ", style(">").with(UI::ACCENT).bold());
        let _ = io::stdout().flush();

        match ag.chat(&input) {
            Ok(_) => {
                println!();
            }
            Err(e) => {
                println!("\rError: {e}");
            }
        }
    }
}

/// Demo mode: show all tools
fn run_demo() {
    let config = Config::load_or_default();
    UI::draw_box("SapNi Tools (Rust)", &[]);
    println!();
    println!("Provider: {}", config.llm.provider);
    println!("Model:    {}", config.llm.model);
    println!("Base URL: {}\n", config.effective_base_url());

    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(Tools::LS::LsTool),
        Box::new(Tools::WRITE::WriteTool),
        Box::new(Tools::EDIT::EditTool),
        Box::new(Tools::DEL::DelTool),
        Box::new(Tools::EXEC::ExecTool),
        Box::new(Tools::CHECK::CheckTool),
        Box::new(Tools::PLUGS::PlugsTool),
        Box::new(Tools::Todo::TodoTool),
    ];

    for tool in &tools {
        println!("{} — {}", 
            style(tool.name()).with(UI::ACCENT).bold(),
            tool.description()
        );
        let schema = tool.schema();
        println!("  Schema: {} params", schema["function"]["parameters"]["properties"].as_object().map(|o| o.len()).unwrap_or(0));
    }

    println!("\n--- Demo: LS ---");
    let mut params = HashMap::new();
    params.insert("path".into(), ".".into());
    let result = tools[0].execute(&params);
    println!("  success={}  msg={}", result.success, result.message);
    if let Some(data) = &result.data {
        println!("  data:\n{}", data);
    }

    println!("\n--- Demo: Agent (requires API key) ---");
    agent_demo(&config, tools);
}

fn agent_demo(config: &Config, tools: Vec<Box<dyn Tool>>) {
    if config.is_api_key_placeholder() {
        UI::warn("Skip: API key not configured");
        return;
    }

    let mut agent = Agent::Agent::new(config.clone(), tools);
    match agent.chat("Introduce yourself in one sentence") {
        Ok(reply) => println!("  Agent: {}", strip_thinking_tag(&reply)),
        Err(e) => println!("  Error: {e}"),
    }
}

/// Handle slash commands
fn handle_command(
    input: &str,
    config: &mut Config,
    agent: &mut Option<Agent::Agent>,
    tools: &[Box<dyn Tool>],
) {
    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    let cmd = parts[0].trim_start_matches('/').to_lowercase();
    let rest = parts.get(1).map(|s| *s).unwrap_or("");
    let _rest2 = parts.get(2).map(|s| *s).unwrap_or("");

    match cmd.as_str() {
        "help" | "?" => {
            UI::print_help();
        }

        "key" => {
            if rest.is_empty() {
                UI::warn("Usage: /key YOUR_API_KEY");
                return;
            }
            config.set_api_key(rest);
            if let Some(ag) = agent {
                ag.llm.set_api_key(config.llm.api_key.clone());
            }
            save_config(config);
            UI::success("API Key set and saved");
        }

        "model" => {
            if rest.is_empty() {
                println!("Current model: {}", config.llm.model);
                return;
            }
            config.set_model(rest);
            save_config(config);
            UI::success(&format!("Model: {} (saved)", config.llm.model));
        }

        "provider" => {
            if rest.is_empty() {
                println!("Current provider: {}", config.llm.provider);
                println!("Available: ollama, deepseek, moonshot, openai, openrouter");
                return;
            }
            match config.set_provider(rest) {
                Ok(_) => {
                    if let Some(ag) = agent {
                        ag.llm.set_base_url(config.llm.base_url.clone());
                    }
                    save_config(config);
                    UI::success(&format!("Provider: {} | Model: {} (saved)", config.llm.provider, config.llm.model));
                }
                Err(e) => UI::error(&e),
            }
        }

        "url" => {
            if rest.is_empty() {
                println!("Current URL: {}", config.effective_base_url());
                return;
            }
            config.set_base_url(rest);
            if let Some(ag) = agent {
                ag.llm.set_base_url(config.llm.base_url.clone());
            }
            save_config(config);
            UI::success(&format!("Base URL: {} (saved)", config.llm.base_url));
        }

        "temp" => {
            if rest.is_empty() {
                println!("Temperature: {}", config.llm.temperature);
                return;
            }
            if let Ok(t) = rest.parse::<f64>() {
                config.set_temperature(t);
                save_config(config);
                UI::success(&format!("Temp: {} (saved)", config.llm.temperature));
            } else {
                UI::error(&format!("Invalid: {rest}"));
            }
        }

        "topp" => {
            if rest.is_empty() {
                println!("TopP: {}", config.llm.top_p);
                return;
            }
            if let Ok(p) = rest.parse::<f64>() {
                config.set_top_p(p);
                save_config(config);
                UI::success(&format!("TopP: {} (saved)", config.llm.top_p));
            } else {
                UI::error(&format!("Invalid: {rest}"));
            }
        }

        "tokens" => {
            if rest.is_empty() {
                println!("Max tokens: {}", config.llm.max_tokens);
                return;
            }
            if let Ok(n) = rest.parse::<u32>() {
                config.set_max_tokens(n);
                save_config(config);
                UI::success(&format!("Max tokens: {} (saved)", config.llm.max_tokens));
            } else {
                UI::error(&format!("Invalid: {rest}"));
            }
        }

        "config" => {
            UI::draw_box(
                "Config",
                &[
                    &format!("Provider:  {}", config.llm.provider),
                    &format!("Model:     {}", config.llm.model),
                    &format!("Base URL:  {}", config.effective_base_url()),
                    &format!("API Key:   {}", if config.is_api_key_placeholder() { "(unset)" } else { "***" }),
                    &format!("Temp:      {}", config.llm.temperature),
                    &format!("TopP:      {}", config.llm.top_p),
                    &format!("MaxTokens: {}", config.llm.max_tokens),
                    &format!("Window:    {}", config.llm.context_window),
                    &format!("Tools:     {}", if config.tools.enabled { "enabled" } else { "disabled" }),
                ],
            );
        }

        "tools" => {
            let tool_list: Vec<(String, String)> = tools
                .iter()
                .map(|t| (t.name().to_string(), t.description().to_string()))
                .collect();
            UI::draw_box("Tools", &[]);
            UI::print_tools(&tool_list);
        }

        "new" | "reset" => {
            if let Some(ag) = agent {
                ag.reset();
                UI::success("Session reset");
            } else {
                UI::warn("(no active session)");
            }
        }

        "quit" | "exit" | "q" => {
            println!("Goodbye.");
            std::process::exit(0);
        }

        _ => {
            UI::error(&format!("Unknown command: {cmd}. Try /help"));
        }
    }
}

fn create_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(Tools::LS::LsTool),
        Box::new(Tools::WRITE::WriteTool),
        Box::new(Tools::EDIT::EditTool),
        Box::new(Tools::DEL::DelTool),
        Box::new(Tools::EXEC::ExecTool),
        Box::new(Tools::CHECK::CheckTool),
        Box::new(Tools::PLUGS::PlugsTool),
        Box::new(Tools::Todo::TodoTool),
    ]
}

/// Strip <thinking> tags from model output
fn strip_thinking_tag(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<thinking>") {
        if let Some(end) = result[start..].find("</thinking>") {
            let end_abs = start + end + "</thinking>".len();
            result.replace_range(start..end_abs, "");
        } else {
            break;
        }
    }
    result.trim().to_string()
}

/// Save config to file
fn save_config(config: &Config) {
    let path = Config::default_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = config.save(path.to_str().unwrap_or(".sapni.json")) {
        UI::error(&format!("Failed to save config: {e}"));
    }
}
