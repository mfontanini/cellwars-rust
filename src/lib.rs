//! This is the official Rust Bot SDK for [cellwars](https://cellwars.io)

use std::sync::{
    Arc,
    Mutex,
};
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::io::{
    BufRead,
    Write,
    Stdin,
    Stdout,
};
use std::mem::take;
use std::str::FromStr;
use std::error::Error;

#[derive(Debug)]
struct ParseCommandError {
    details: &'static str,
}

impl ParseCommandError {
    fn new(details: &'static str) -> Self {
        Self {
            details,
        }
    }
}

impl fmt::Display for ParseCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to parse command: {}", self.details)
    }
}

impl Error for ParseCommandError {

}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Initialize{ width: u32, height: u32, team_id: u32, my_column: u32, enemy_column: u32 },
    Spawn{ cell_id: u32, x: u32, y: u32, health: u32, team_id: u32, age: u32 },
    Die{ cell_id: u32 },
    SetCellProperties{ cell_id: u32, x: u32, y: u32, health: u32, age: u32 },
    ConflictingActions{ x: u32, y: u32 },
    RunRound,
    EndGame,
}

#[derive(Debug, Eq, PartialEq)]
enum Action {
    Move{ cell_id: u32, x: u32, y: u32 },
    Attack{ cell_id: u32, x: u32, y: u32 },
    Explode{ cell_id: u32 },
    Initialized,
    RoundEnd,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Move{ cell_id, x, y } =>
                write!(f, "MOVE {} {} {}", cell_id, x, y ),
            Action::Attack{ cell_id, x, y } =>
                write!(f, "ATTACK {} {} {}", cell_id, x, y ),
            Action::Explode{ cell_id } =>
                write!(f, "EXPLODE {}", cell_id ),
            Action::Initialized => write!(f, "INITIALIZED"),
            Action::RoundEnd => write!(f, "ROUND_END"),
        }
    }
}

impl FromStr for Command {
    type Err = ParseCommandError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tokens : Vec<&str> = s.trim().split(' ').collect();
        if tokens.len() == 0 {
            return Err(ParseCommandError::new("no tokens in command"));
        }
        let mut parameters = Vec::new();
        for token in tokens.iter().skip(1) {
            let token = match token.parse::<u32>() {
                Ok(token) => token,
                Err(_) => return Err(ParseCommandError::new("non int parameter found")),
            };
            parameters.push(token);
        }
        match (tokens[0], parameters.len()) {
            ("INITIALIZE", 5) => Ok(Command::Initialize{
                width: parameters[0],
                height: parameters[1],
                team_id: parameters[2],
                my_column: parameters[3],
                enemy_column: parameters[4],
            }),
            ("SPAWN", 6) => Ok(Command::Spawn{
                cell_id: parameters[0],
                x: parameters[1],
                y: parameters[2],
                health: parameters[3],
                team_id: parameters[4],
                age: parameters[5],
            }),
            ("DIE", 1) => Ok(Command::Die{
                cell_id: parameters[0],
            }),
            ("SET_CELL_PROPERTIES", 5) => Ok(Command::SetCellProperties{
                cell_id: parameters[0],
                x: parameters[1],
                y: parameters[2],
                health: parameters[3],
                age: parameters[4],
            }),
            ("CONFLICTING_ACTIONS", 2) => Ok(Command::ConflictingActions{
                x: parameters[0],
                y: parameters[1],
            }),
            ("RUN_ROUND", 0) => Ok(Command::RunRound),
            ("END_GAME", 0) => Ok(Command::EndGame),
            _ => Err(ParseCommandError::new("Unknown command")),
        }
    }
}

struct CommunicatorDetails {
    input: io::BufReader<Stdin>,
    output: io::BufWriter<Stdout>,
    pending_actions: Vec<Action>,
}

#[doc(hidden)]
pub struct Communicator {
    details: Mutex<CommunicatorDetails>,
}

impl Communicator {
    pub fn new(input: Stdin, output: Stdout) -> Self {
        Self {
            details: Mutex::new(CommunicatorDetails{
                input: io::BufReader::new(input),
                output: io::BufWriter::new(output),
                pending_actions: Vec::new(),
            })
        }
    }

    fn add_action(&self, action: Action) {
        self.details.lock().unwrap().pending_actions.push(action);
    }

    fn send_action(
        action: Action,
        details: &mut CommunicatorDetails,
    ) -> io::Result<()>
    {
        details.output.write_all(format!("{}\n", action).as_bytes())?;
        Ok(())
    }

    fn flush(details: &mut CommunicatorDetails) -> io::Result<()> {
        details.output.flush()?;
        Ok(())
    }

    fn flush_action(&self, action: Action) -> io::Result<()> {
        let mut details = self.details.lock().unwrap();
        Self::send_action(action, &mut details)?;
        Self::flush(&mut details)?;
        Ok(())
    }

    fn end_round(&self) -> io::Result<()> {
        let mut details = self.details.lock().unwrap();
        let mut pending_actions = take(&mut details.pending_actions);
        pending_actions.push(Action::RoundEnd);
        for action in pending_actions.into_iter() {
            Self::send_action(action, &mut details)?;
        }
        Self::flush(&mut details)?;
        Ok(())
    }

    fn read_command(&self) -> Result<Command, Box<dyn Error>> {
        let mut line = String::new();
        self.details.lock().unwrap().input.read_line(&mut line)?;
        let command: Command = line.parse()?;
        Ok(command)
    }
}

/// A position, defined by a tuple of x and y coordinates.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    x: i32,
    y: i32,
}

impl From<(i32, i32)> for Position {
    fn from((x, y): (i32, i32)) -> Position {
        Position {
            x,
            y,
        }
    }
}

impl Position {
    /// The x coordinate of this position.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// The y coordinate of this position.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Translate this position by an offset.
    ///
    /// # Arguments
    ///
    /// * `x` - the x offset.
    /// * `y` - the y offset.
    pub fn translated_by_offset(&self, x: i32, y: i32) -> Position {
        Position {
            x: self.x + x,
            y: self.y + y,
        }
    }

    /// Translates this position by the given direction.
    ///
    /// # Arguments
    ///
    /// * `direction` - the direction to translate this position by.
    pub fn translated_by_direction(&self, direction: &Direction) -> Position {
        let offset = direction.as_position_offset();
        self.translated_by_offset(offset.0, offset.1)
    }

    /// The distance between this position and another one.
    ///
    /// Note that this is going to be the Manhattan distance between the two positions.
    ///
    /// # Arguments
    ///
    /// * `position` - the position to calculate the distance against.
    pub fn distance(&self, position: &Position) -> u64 {
        ((self.x - position.x).abs() + (self.y - position.y).abs()) as u64
    }
}

/// A direction.
pub enum Direction {
    /// North (up)
    North,
    /// South (down)
    South,
    /// East (right)
    East,
    /// West (left)
    West,
}

impl Direction {
    fn as_position_offset(&self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::South => (0, 1),
            Direction::East => (1, 0),
            Direction::West => (-1, 0),
        }
    }
}

/// A cell in the game
pub struct Cell {
    cell_id: u32,
    position: Position,
    health: u32,
    team_id: u32,
    age: u32,
    is_enemy: bool,
    communicator: Arc<Communicator>,
    world_properties: WorldProperties,
}

impl Cell {
    /// Gets an unique identifier for this cell. The identifier will not change for a given cell for the
    /// duration of each match.
    pub fn cell_id(&self) -> u32 {
        self.cell_id
    }

    /// Gets the position of this cell.
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Gets the health of this cell. This is typically a number between 1 and 100.
    pub fn health(&self) -> u32 {
        self.health
    }

    /// Gets this cell's team identifier.
    pub fn team_id(&self) -> u32 {
        self.team_id
    }

    /// Gets the cell's age. Every time a new chunk of cells spawn, all new cells will have an age of 0
    /// and all of the existing cell's ages will be incremented by 1.
    pub fn age(&self) -> u32 {
        self.age
    }

    /// Indicates if this cell belongs to the enemy. This is a shorthand for checking if the team_id is
    /// different from yours.
    pub fn is_enemy(&self) -> bool {
        self.is_enemy
    }

    fn is_in_bounds(&self, position: &Position) -> bool {
        position.x() >= 0 &&
            position.y() >= 0 &&
            position.x() < self.world_properties.width as i32 &&
            position.y() < self.world_properties.height as i32
    }

    /// Indicates if the cell can move into the given position.
    ///
    /// # Arguments
    ///
    /// * `position` - the target position.
    pub fn can_move_to_position(&self, position: &Position) -> bool {
        self.is_in_bounds(position) && self.position.distance(position) == 1
    }

    /// Indicates if the cell can move in the given direction.
    ///
    /// # Arguments
    ///
    /// * `direction` - the direction.
    pub fn can_move_in_direction(&self, direction: &Direction) -> bool {
        self.can_move_to_position(&self.position.translated_by_direction(direction))
    }

    /// Indicates if the cell can attack the given position.
    ///
    /// # Arguments
    ///
    /// * `position` - the target position.
    pub fn can_attack_position(&self, position: &Position) -> bool {
        self.is_in_bounds(position) &&
            (position.x() - self.position.x()).abs() <= 1 &&
            (position.y() - self.position.y()).abs() <= 1
    }

    /// Indicates if the cell can attack the given cell.
    ///
    /// Note that this is purely a proximity check. e.g. this does not take into account whether the cell is an enemy.
    ///
    /// # Arguments
    ///
    /// * `cell` - the target cell.
    pub fn can_attack_cell(&self, cell: &Cell) -> bool {
        self.can_attack_position(&cell.position)
    }

    /// Instructs this cell to attack the given cell.
    ///
    /// See the documentation on the restrictions on attacking too-far-away positions.
    ///
    /// # Arguments
    ///
    /// * `cell` - the cell to be attacked.
    pub fn attack_cell(&self, cell: &Cell) {
        self.attack_position(&cell.position);
    }

    /// Instructs this cell to attack the given position.
    ///
    /// See the documentation on the restrictions on attacking too-far-away positions.
    ///
    /// # Arguments
    ///
    /// * `position` - the position to be attacked.
    pub fn attack_position(&self, position: &Position) {
        if self.can_attack_position(position) {
            self.communicator.add_action(Action::Attack{
                cell_id: self.cell_id,
                x: position.x() as u32,
                y: position.y() as u32,
            });
        }
    }

    /// Instructs this cell to move into the given position.
    ///
    /// See the documentation on the restrictions on moving into too-far-away positions and movement conflicts.
    ///
    /// # Arguments
    ///
    /// * `position` - the position to move into.
    pub fn move_to_position(&self, position: &Position) {
        if self.can_move_to_position(position) {
            self.communicator.add_action(Action::Move{
                cell_id: self.cell_id,
                x: position.x() as u32,
                y: position.y() as u32,
            });
        }
    }

    /// Instructs this cell to move into the given direction.
    ///
    /// See the documentation on movement conflicts.
    ///
    /// # Arguments
    ///
    /// * `direction` - the direction to move into.
    pub fn move_in_direction(&self, direction: &Direction) {
        if self.can_move_in_direction(direction) {
            self.move_to_position(&self.position.translated_by_direction(&direction));
        }
    }

    /// Instructs this cell to explode.
    ///
    /// This will cause your cell to die and cause damage to *all* surrounding cells.
    pub fn explode(&self) {
        self.communicator.add_action(Action::Explode{cell_id: self.cell_id});
    }
}

impl fmt::Debug for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cell(cell_id: {}, position: {:?}, team_id: {}, health: {})",
            self.cell_id,
            &self.position,
            self.team_id,
            self.health
        )
    }
}

#[derive(Default, Clone)]
struct WorldProperties {
    width: u32,
    height: u32,
    my_team_id: u32,
    my_column: u32,
    enemy_column: u32,
}

/// The state of the game's world.
#[derive(Default)]
pub struct WorldState {
    cells: HashMap<u32, Cell>,
    properties: WorldProperties,
}

impl WorldState {
    /// The width of the world, in number of squares.
    pub fn width(&self) -> u32 {
        self.properties.width
    }

    /// The height of the world, in number of squares.
    pub fn height(&self) -> u32 {
        self.properties.height
    }

    /// The identifier of your bot's team. Use this to differentiate between your cells and your enemy's.
    pub fn my_team_id(&self) -> u32 {
        self.properties.my_team_id
    }

    /// The index of the column in which your cells spawn. This will typically be either 0 or width - 1.
    pub fn my_starting_column(&self) -> u32 {
        self.properties.my_column
    }

    /// The index of the column in which your enemy's cells spawn. This will typically be either 0 or width - 1.
    pub fn enemy_starting_column(&self) -> u32 {
        self.properties.enemy_column
    }

    /// Gets all of the cells you currently control
    pub fn my_cells(&self) -> Vec<&Cell> {
        self.cells.values().filter(|cell| cell.team_id == self.my_team_id()).collect()
    }

    /// Gets all of the cells the enemy currently controls
    pub fn enemy_cells(&self) -> Vec<&Cell> {
        self.cells.values().filter(|cell| cell.team_id != self.my_team_id()).collect()
    }
}

#[doc(hidden)]
pub struct GameCoordinator {
    communicator: Arc<Communicator>,
}

impl GameCoordinator {
    pub fn new(communicator: Communicator) -> Self {
        Self {
            communicator: Arc::new(communicator),
        }
    }

    fn apply_initialize(
        width: u32,
        height: u32,
        my_team_id: u32,
        my_column: u32,
        enemy_column: u32,
    ) -> WorldState
    {
        WorldState {
            properties: WorldProperties{
                width,
                height,
                my_team_id,
                my_column,
                enemy_column,
            },
            ..Default::default()
        }
    }

    fn apply_spawn(
        &self,
        mut world_state: WorldState,
        cell_id: u32,
        x: u32,
        y: u32,
        health: u32,
        team_id: u32,
        age: u32,
    ) -> WorldState
    {
        let is_enemy = team_id != world_state.my_team_id();
        world_state.cells.insert(cell_id, Cell{
            cell_id,
            position: Position{x: x as i32, y: y as i32},
            health,
            team_id,
            age,
            is_enemy,
            communicator: self.communicator.clone(),
            world_properties: world_state.properties.clone(),
        });
        world_state
    }

    fn apply_set_cell_properties(
        mut world_state: WorldState,
        cell_id: u32,
        x: u32,
        y: u32,
        health: u32,
        age: u32,
    ) -> WorldState
    {
        let cell = world_state.cells.get_mut(&cell_id).expect("Invalid cell id");
        cell.position = Position{x: x as i32, y: y as i32};
        cell.health = health;
        cell.age = age;
        world_state
    }

    fn apply_die(mut world_state: WorldState, cell_id: u32) -> WorldState {
        world_state.cells.remove(&cell_id);
        world_state
    }

    fn advertise_initialization(&self) -> io::Result<()> {
        self.communicator.flush_action(Action::Initialized)?;
        Ok(())
    }

    fn apply_command(
        &self,
        command: Command,
        world_state: WorldState,
    ) -> WorldState
    {
        let world_state = match command {
            Command::Initialize{ width, height, team_id, my_column, enemy_column } =>
                Self::apply_initialize(width, height, team_id, my_column, enemy_column),
            Command::Spawn{ cell_id, x, y, health, team_id, age} =>
                self.apply_spawn(world_state, cell_id, x, y, health, team_id, age),
            Command::Die{ cell_id } =>
                Self::apply_die(world_state, cell_id),
            Command::SetCellProperties{ cell_id, x, y, health, age } =>
                Self::apply_set_cell_properties(world_state, cell_id, x, y, health, age),
            _ => world_state,
        };
        world_state
    }

    pub fn run_loop<B>(&self, mut bot: B) -> Result<(), Box<dyn Error>>
    where
        B: UserBot,
    {
        let mut world_state = WorldState::default();
        self.advertise_initialization()?;
        loop {
            let mut command = self.communicator.read_command()?;
            if command == Command::EndGame {
                break;
            }
            while command != Command::RunRound {
                world_state = self.apply_command(command, world_state);
                command = self.communicator.read_command()?;
            }
            bot.run_round(&world_state);
            self.communicator.end_round()?;
        }
        Ok(())
    }
}

/// A bot defined by the user
///
/// This trait should be implemented by users and should contain all of their bot's logic
pub trait UserBot {
    /// Run a particular round of the game.
    ///
    /// The implementation of this method must guarantee that at most one action is emitted per
    /// cell.
    ///
    /// # Arguments
    ///
    /// * `world_state` - the current state of the world
    fn run_round(&mut self, world_state: &WorldState);
}
