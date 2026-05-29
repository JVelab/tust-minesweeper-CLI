use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum CellState {
    Hidden,
    Revealed,
    Flagged,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Cell {
    state: CellState,
    is_mine: bool,
    adjacent_mines: u8,
}

impl Cell {
    fn new() -> Self {
        Cell {
            state: CellState::Hidden,
            is_mine: false,
            adjacent_mines: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GameState {
    Menu,
    Playing,
    Won,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Difficulty {
    pub rows: u32,
    pub cols: u32,
    pub mines: u32,
}

impl Difficulty {
    fn easy() -> Self {
        Difficulty { rows: 9, cols: 9, mines: 10 }
    }
    fn medium() -> Self {
        Difficulty { rows: 16, cols: 16, mines: 40 }
    }
    fn hard() -> Self {
        Difficulty { rows: 16, cols: 30, mines: 99 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub difficulty: Difficulty,
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig {
            difficulty: Difficulty::easy(),
        }
    }
}

struct Game {
    state: GameState,
    board: Vec<Vec<Cell>>,
    difficulty: Difficulty,
    cursor_x: usize,
    cursor_y: usize,
    flags_remaining: i32,
    timer_seconds: u32,
    game_started: Option<Instant>,
    first_click: bool,
}

impl Game {
    fn new(difficulty: Difficulty) -> Self {
        let rows = difficulty.rows as usize;
        let cols = difficulty.cols as usize;
        Game {
            state: GameState::Menu,
            board: vec![vec![Cell::new(); cols]; rows],
            difficulty,
            cursor_x: cols / 2,
            cursor_y: rows / 2,
            flags_remaining: difficulty.mines as i32,
            timer_seconds: 0,
            game_started: None,
            first_click: true,
        }
    }

    fn reset(&mut self) {
        let difficulty = self.difficulty;
        *self = Self::new(difficulty);
    }

    fn place_mines(&mut self, safe_x: usize, safe_y: usize) {
        let rows = self.difficulty.rows as usize;
        let cols = self.difficulty.cols as usize;
        let mut positions: Vec<(usize, usize)> = Vec::new();

        for y in 0..rows {
            for x in 0..cols {
                let is_safe = x >= safe_x.saturating_sub(1) && x <= safe_x + 1 &&
                              y >= safe_y.saturating_sub(1) && y <= safe_y + 1;
                if !is_safe {
                    positions.push((x, y));
                }
            }
        }

        positions.shuffle(&mut rand::thread_rng());
        let mines_to_place = self.difficulty.mines as usize;

        for i in 0..mines_to_place.min(positions.len()) {
            let (x, y) = positions[i];
            self.board[y][x].is_mine = true;
        }

        for y in 0..rows {
            for x in 0..cols {
                if !self.board[y][x].is_mine {
                    self.board[y][x].adjacent_mines = self.count_adjacent_mines(x, y);
                }
            }
        }
    }

    fn count_adjacent_mines(&self, x: usize, y: usize) -> u8 {
        let mut count = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 { continue; }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && nx < self.difficulty.cols as i32 &&
                   ny >= 0 && ny < self.difficulty.rows as i32 {
                    let nx = nx as usize;
                    let ny = ny as usize;
                    if self.board[ny][nx].is_mine {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn reveal_cell(&mut self, x: usize, y: usize) {
        if x >= self.difficulty.cols as usize || y >= self.difficulty.rows as usize {
            return;
        }

        {
            let cell = &self.board[y][x];
            if cell.state == CellState::Revealed || cell.state == CellState::Flagged {
                return;
            }
        }

        if self.first_click {
            self.first_click = false;
            self.place_mines(x, y);
            self.game_started = Some(Instant::now());
        }

        let is_mine = self.board[y][x].is_mine;
        self.board[y][x].state = CellState::Revealed;

        if is_mine {
            self.state = GameState::Lost;
            self.reveal_all_mines();
            return;
        }

        if self.board[y][x].adjacent_mines == 0 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 { continue; }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < self.difficulty.cols as i32 &&
                       ny >= 0 && ny < self.difficulty.rows as i32 {
                        self.reveal_cell(nx as usize, ny as usize);
                    }
                }
            }
        }

        self.check_win();
    }

    fn toggle_flag(&mut self, x: usize, y: usize) {
        if x >= self.difficulty.cols as usize || y >= self.difficulty.rows as usize {
            return;
        }

        match self.board[y][x].state {
            CellState::Hidden => {
                self.board[y][x].state = CellState::Flagged;
                self.flags_remaining -= 1;
            },
            CellState::Flagged => {
                self.board[y][x].state = CellState::Hidden;
                self.flags_remaining += 1;
            },
            _ => {}
        }
    }

    fn reveal_all_mines(&mut self) {
        for row in &mut self.board {
            for cell in row {
                if cell.is_mine && cell.state != CellState::Flagged {
                    cell.state = CellState::Revealed;
                }
            }
        }
    }

    fn check_win(&mut self) {
        for row in &self.board {
            for cell in row {
                if !cell.is_mine && cell.state != CellState::Revealed {
                    return;
                }
            }
        }
        self.state = GameState::Won;
        for row in &mut self.board {
            for cell in row {
                if cell.is_mine && cell.state != CellState::Flagged {
                    cell.state = CellState::Flagged;
                }
            }
        }
        self.flags_remaining = 0;
    }

    fn update_timer(&mut self) {
        if let Some(start) = self.game_started {
            self.timer_seconds = start.elapsed().as_secs() as u32;
        }
    }
}

fn get_config_path() -> PathBuf {
    let mut path = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    path.push("minesweeper_config.json");
    path
}

fn load_config() -> GameConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    GameConfig::default()
}

fn save_config(config: &GameConfig) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}

fn get_color_for_number(num: u8) -> Color {
    match num {
        1 => Color::Blue,
        2 => Color::Green,
        3 => Color::Red,
        4 => Color::Magenta,
        5 => Color::Yellow,
        6 => Color::Cyan,
        7 => Color::White,
        8 => Color::Gray,
        _ => Color::Reset,
    }
}

fn draw_menu(frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ])
        .split(area);

    let title = Paragraph::new("BUSCAMINAS")
        .style(Style::default().fg(Color::Yellow).bold())
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    let menu_style = Style::default().fg(Color::Cyan);

    let items = vec![
        "1. Facil (9x9 - 10 minas)",
        "2. Medio (16x16 - 40 minas)",
        "3. Dificil (30x16 - 99 minas)",
        "4. Salir",
    ];

    for (i, text) in items.iter().enumerate() {
        let item = Paragraph::new(*text)
            .style(menu_style)
            .alignment(Alignment::Center);
        frame.render_widget(item, chunks[i + 1]);
    }
}

fn draw_game(frame: &mut Frame, area: Rect, game: &Game) {
    let header_height = 4;
    let footer_height = 3;

    let header_area = Rect::new(area.x, area.y, area.width, header_height);
    let footer_area = Rect::new(area.x, area.y + area.height - footer_height, area.width, footer_height);
    let main_area = Rect::new(area.x, area.y + header_height, area.width, area.height - header_height - footer_height);

    let block = Block::default()
        .title(" Buscaminas ")
        .title_style(Style::default().fg(Color::Yellow).bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(header_area);

    let mines_display = format!("Minas: {}", game.flags_remaining);
    let mines_para = Paragraph::new(mines_display)
        .style(Style::default().fg(Color::Red).bold())
        .alignment(Alignment::Center);
    frame.render_widget(mines_para, header_chunks[0]);

    let state_text = match game.state {
        GameState::Playing => "[ Jugando ]",
        GameState::Won => "[ Ganaste! ]",
        GameState::Lost => "[ Perdiste ]",
        _ => "",
    };
    let state_style = match game.state {
        GameState::Won => Style::default().fg(Color::Green).bold(),
        GameState::Lost => Style::default().fg(Color::Red).bold(),
        _ => Style::default().fg(Color::Cyan).bold(),
    };
    let state_para = Paragraph::new(state_text)
        .style(state_style)
        .alignment(Alignment::Center);
    frame.render_widget(state_para, header_chunks[1]);

    let timer_display = format!("Tiempo: {:03}s", game.timer_seconds);
    let timer_para = Paragraph::new(timer_display)
        .style(Style::default().fg(Color::Blue).bold())
        .alignment(Alignment::Center);
    frame.render_widget(timer_para, header_chunks[2]);

    let inner = main_area;
    let cols = game.difficulty.cols as u16;
    let rows = game.difficulty.rows as u16;
    let cell_width = (inner.width / cols).max(1);
    let cell_height = (inner.height / rows).max(1);

    for y in 0..game.difficulty.rows as usize {
        for x in 0..game.difficulty.cols as usize {
            let cell_area = Rect::new(
                inner.x + x as u16 * cell_width,
                inner.y + y as u16 * cell_height,
                cell_width,
                cell_height,
            );

            let cell = &game.board[y][x];
            let is_cursor = x == game.cursor_x && y == game.cursor_y;

            let (symbol, fg_color, bg_color) = match cell.state {
                CellState::Hidden => {
                    if is_cursor { (String::from("#"), Color::Black, Color::Yellow) }
                    else { (String::from("."), Color::DarkGray, Color::Reset) }
                },
                CellState::Flagged => (String::from("!"), Color::Red, Color::Reset),
                CellState::Revealed => {
                    if cell.is_mine { (String::from("*"), Color::Red, Color::DarkGray) }
                    else if cell.adjacent_mines > 0 {
                        let c = get_color_for_number(cell.adjacent_mines);
                        (format!("{}", cell.adjacent_mines), c, Color::Reset)
                    }
                    else { (String::from(" "), Color::Reset, Color::Reset) }
                },
            };

            let mut cell_style = Style::default()
                .fg(fg_color)
                .bg(bg_color);
            if is_cursor {
                cell_style = cell_style.bold();
            }

            let para = Paragraph::new(symbol)
                .style(cell_style)
                .alignment(Alignment::Center);
            frame.render_widget(para, cell_area);
        }
    }

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(footer_area);

    let controls = vec![
        "WASD: Mover",
        "Enter: Revelar",
        "F: Bandera",
        "R: Reiniciar",
        "Q: Menu",
    ];

    for (i, text) in controls.iter().enumerate() {
        let para = Paragraph::new(*text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(para, footer_chunks[i]);
    }
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new(Difficulty::easy());
    let mut config = load_config();

    let result = run_game(&mut terminal, &mut game, &mut config);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    game: &mut Game,
    config: &mut GameConfig,
) -> Result<(), io::Error> {
    loop {
        terminal.draw(|frame| {
            let size = frame.size();
            match game.state {
                GameState::Menu => draw_menu(frame, size),
                _ => draw_game(frame, size, game),
            }
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press { continue; }

            match game.state {
                GameState::Menu => {
                    match key.code {
                        KeyCode::Char('1') => {
                            game.difficulty = Difficulty::easy();
                            game.reset();
                            game.state = GameState::Playing;
                        },
                        KeyCode::Char('2') => {
                            game.difficulty = Difficulty::medium();
                            game.reset();
                            game.state = GameState::Playing;
                        },
                        KeyCode::Char('3') => {
                            game.difficulty = Difficulty::hard();
                            game.reset();
                            game.state = GameState::Playing;
                        },
                        KeyCode::Char('4') | KeyCode::Esc => {
                            save_config(config);
                            return Ok(());
                        },
                        _ => {}
                    }
                },
                GameState::Playing => {
                    match key.code {
                        KeyCode::Char('w') | KeyCode::Up => {
                            if game.cursor_y > 0 { game.cursor_y -= 1; }
                        },
                        KeyCode::Char('s') | KeyCode::Down => {
                            if game.cursor_y < game.difficulty.rows as usize - 1 { game.cursor_y += 1; }
                        },
                        KeyCode::Char('a') | KeyCode::Left => {
                            if game.cursor_x > 0 { game.cursor_x -= 1; }
                        },
                        KeyCode::Char('d') | KeyCode::Right => {
                            if game.cursor_x < game.difficulty.cols as usize - 1 { game.cursor_x += 1; }
                        },
                        KeyCode::Enter => {
                            if game.board[game.cursor_y][game.cursor_x].state != CellState::Revealed {
                                game.reveal_cell(game.cursor_x, game.cursor_y);
                            }
                        },
                        KeyCode::Char(' ') => {
                            game.toggle_flag(game.cursor_x, game.cursor_y);
                        },
                        KeyCode::Char('f') | KeyCode::Char('F') => {
                            game.toggle_flag(game.cursor_x, game.cursor_y);
                        },
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            game.reset();
                            game.state = GameState::Playing;
                        },
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            game.state = GameState::Menu;
                        },
                        _ => {}
                    }
                    game.update_timer();
                },
                GameState::Won | GameState::Lost => {
                    match key.code {
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            game.reset();
                            game.state = GameState::Playing;
                        },
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            game.state = GameState::Menu;
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}