use clap::Parser;
use color_eyre::{config::HookBuilder, eyre, Result};
use crossterm::{
    event,
    event::KeyCode,
    event::{Event, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    buffer::{Buffer, Cell},
    layout::{Constraint, Layout, Rect},
    style::Color,
    terminal::Terminal,
    text::Text,
    widgets::Widget,
};
use std::{
    io::stdout,
    panic,
    time::{Duration, Instant},
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 0.0)]
    fps: f64,
}

#[derive(Debug)]
struct App {
    args: Args,
    state: AppState,
    fps_widget: FpsWidget,
    game_of_life: GameOfLifeWidget,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Quit,
}

#[derive(Debug)]
struct FpsWidget {
    frame_count: usize,
    last_instant: Instant,
    fps: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GridCell {
    Dead = 0,
    Alive = 1,
}

impl GridCell {
    fn into(&self) -> u8 {
        match self {
            GridCell::Dead => 0,
            GridCell::Alive => 1,
        }
    }

    fn render(&self, cell: &mut Cell) {
        match self {
            GridCell::Alive => cell
                .set_fg(Color::Rgb(255, 255, 255))
                .set_bg(Color::Rgb(255, 255, 255)),
            GridCell::Dead => cell.set_fg(Color::Rgb(0, 0, 0)).set_bg(Color::Rgb(0, 0, 0)),
        };
    }
}

#[derive(Debug, Default)]
struct GameOfLifeWidget {
    grid: Option<(Vec<GridCell>, Vec<GridCell>)>,
    diff: Option<u32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    install_error_hooks()?;
    let terminal = init_terminal()?;
    App::new(args).run(terminal)?;
    restore_terminal()?;
    Ok(())
}

impl App {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            state: AppState::default(),
            fps_widget: FpsWidget::default(),
            game_of_life: GameOfLifeWidget::default(),
        }
    }

    pub fn run(mut self, mut terminal: Terminal<impl Backend>) -> Result<()> {
        while self.is_running() {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.size()))?;
            self.handle_events()?;
        }
        Ok(())
    }

    const fn is_running(&self) -> bool {
        matches!(self.state, AppState::Running)
    }

    fn handle_events(&mut self) -> Result<()> {
        let mut timeout = 0.0;
        if self.args.fps != 0.0 {
            timeout = 1.0 / self.args.fps;
        }
        let timeout = Duration::from_secs_f64(timeout);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.state = AppState::Quit;
                };
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('r') {
                    self.game_of_life = GameOfLifeWidget::default();
                };

                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('f')
                    && self.args.fps >= 1.0
                {
                    self.args.fps -= 1.0;
                };
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('s')
                    && self.args.fps < f64::MAX
                {
                    self.args.fps += 1.0;
                };
            }
        }
        Ok(())
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::{Length, Min};
        let [top, area] = Layout::vertical([Length(1), Min(0)]).areas(area);
        let [title, info] = Layout::horizontal([Min(0), Constraint::Percentage(50)]).areas(top);
        let [osc, fps] = Layout::horizontal([Min(0), Constraint::Percentage(50)]).areas(info);
        Text::from("Game of Life. Press q to quit, r to restart")
            .left_aligned()
            .render(title, buf);
        self.fps_widget.render(fps, buf);
        self.game_of_life.render(area, buf);
        self.game_of_life.print_diff(osc, buf);
    }
}

impl Default for FpsWidget {
    fn default() -> Self {
        Self {
            frame_count: 0,
            last_instant: Instant::now(),
            fps: None,
        }
    }
}

impl Widget for &mut FpsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.calculate_fps();
        if let Some(fps) = self.fps {
            let text = format!("{fps:.1} fps");
            Text::from(text).right_aligned().render(area, buf);
        }
    }
}

impl FpsWidget {
    #[allow(clippy::cast_precision_loss)]
    fn calculate_fps(&mut self) {
        self.frame_count += 1;
        let elapsed = self.last_instant.elapsed();
        if elapsed > Duration::from_secs(1) && self.frame_count > 2 {
            self.fps = Some(self.frame_count as f32 / elapsed.as_secs_f32());
            self.frame_count = 0;
            self.last_instant = Instant::now();
        }
    }
}

impl Widget for &mut GameOfLifeWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.calculate_game(area);
        let Some((grid, _)) = self.grid.as_ref() else {
            return;
        };
        let sides = area.left()..area.right();
        let length = sides.len();
        for (xi, x) in sides.enumerate() {
            for (yi, y) in (area.top()..area.bottom()).enumerate() {
                grid[yi * length + xi].render(buf.get_mut(x, y).set_char('▀'));
            }
        }
    }
}

impl GameOfLifeWidget {
    fn print_diff(&mut self, area: Rect, buf: &mut Buffer) {
        let Some(diff) = self.diff.take() else {
            return;
        };
        let text = format!("{diff} blocks changed");
        Text::from(text).left_aligned().render(area, buf);
    }

    #[allow(clippy::cast_precision_loss)]
    fn calculate_game(&mut self, size: Rect) {
        let Rect { width, height, .. } = size;
        let height = height as usize;
        let width = width as usize;
        if self.grid.is_none()
            || self
                .grid
                .as_ref()
                .is_some_and(|(grid, _)| grid.len() != height * width)
        {
            self.generate_game(size);
            return;
        }

        let Some((grid, ref mut cached)) = self.grid.as_mut() else {
            return;
        };

        cached.copy_from_slice(grid);

        let mut diff = 0;
        for (y, row) in grid.chunks_exact_mut(width).enumerate() {
            let len = row.len();
            for (x, cell) in row.iter_mut().enumerate() {
                // Doing this to avoid having to mess with wrapping sub
                // Tested the indexes with the safe alternatives so the
                // limits would be correct.
                let up = if y == 0 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y - 1) * len + x) }
                };
                let upleft = if y == 0 || x == 0 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y - 1) * len + x - 1) }
                };
                let upright = if y == 0 || x == width - 1 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y - 1) * len + x + 1) }
                };
                let down = if y == height - 1 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y + 1) * len + x) }
                };
                let downleft = if y == height - 1 || x == 0 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y + 1) * len + x - 1) }
                };
                let downright = if y == height - 1 || x == width - 1 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked((y + 1) * len + x + 1) }
                };
                let left = if x == 0 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked(y * len + x - 1) }
                };
                let right = if x == width - 1 {
                    &GridCell::Dead
                } else {
                    unsafe { cached.get_unchecked(y * len + x + 1) }
                };

                let cached = unsafe { cached.get_unchecked(y * len + x) };
                let neighbors: u8 = up.into()
                    + down.into()
                    + right.into()
                    + left.into()
                    + upleft.into()
                    + upright.into()
                    + downleft.into()
                    + downright.into();
                // Any live cell with fewer than two live neighbors dies, as if by underpopulation.
                // Any live cell with two or three live neighbors lives on to the next generation.
                // Any live cell with more than three live neighbors dies, as if by overpopulation.
                // Any dead cell with exactly three live neighbors becomes a live cell, as if by reproduction.
                *cell = match (neighbors, cached) {
                    (0..2, GridCell::Dead) => {
                        continue;
                    }
                    (0..2, GridCell::Alive) => GridCell::Dead,
                    (2 | 3, GridCell::Alive) => {
                        continue;
                    }
                    (4.., GridCell::Alive) => GridCell::Dead,
                    (3, GridCell::Dead) => GridCell::Alive,
                    (_, GridCell::Dead) => {
                        continue;
                    }
                };
                diff += 1;
            }
        }
        self.diff = Some(diff);
    }

    fn generate_game(&mut self, size: Rect) {
        let Rect { width, height, .. } = size;
        let height = height as usize;
        let width = width as usize;
        let mut grid = Vec::with_capacity(height * width);
        for _ in 0..height {
            for _ in 0..width {
                if rand::random() {
                    grid.push(GridCell::Alive);
                } else {
                    grid.push(GridCell::Dead);
                }
            }
        }

        self.grid.replace((grid.clone(), grid));
    }
}

fn install_error_hooks() -> Result<()> {
    let (panic, error) = HookBuilder::default().into_hooks();
    let panic = panic.into_panic_hook();
    let error = error.into_eyre_hook();
    eyre::set_hook(Box::new(move |e| {
        let _ = restore_terminal();
        error(e)
    }))?;
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic(info);
    }));
    Ok(())
}

fn init_terminal() -> Result<Terminal<impl Backend>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
