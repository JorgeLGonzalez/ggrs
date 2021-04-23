use crate::circular_buffer::CircularBuffer;
use crate::frame_info::{FrameInfo, GameInput};
use crate::network_stats::NetworkStats;
use crate::player::Player;
use crate::sync_layer::SyncLayer;
use crate::{GGEZError, GGEZInterface, GGEZSession};

/// During a SyncTestSession, GGEZ will simulate a rollback every frame and resimulate the last n states, where n is the given check distance. If you provide checksums
/// in your [GGEZInterface::save_game_state()] function, the SyncTestSession will compare the resimulated checksums with the original checksums and report if there was a mismatch.
#[derive(Debug)]
pub struct SyncTestSession {
    frame: u32,
    num_players: u32,
    input_size: usize,
    check_distance: u32,
    running: bool,
    current_input: GameInput,
    saved_frames: CircularBuffer<FrameInfo>,
    sync_layer: SyncLayer,
}

impl SyncTestSession {
    pub fn new(check_distance: u32, num_players: u32, input_size: usize) -> SyncTestSession {
        SyncTestSession {
            frame: 0,
            num_players,
            input_size,
            check_distance,
            running: false,
            current_input: GameInput::new(input_size * num_players as usize, None),
            saved_frames: CircularBuffer::new(crate::MAX_PREDICTION_FRAMES as usize),
            sync_layer: SyncLayer::new(num_players, input_size),
        }
    }
}

impl GGEZSession for SyncTestSession {
    /// Must be called for each player in the session (e.g. in a 3 player session, must be called 3 times). Returns a playerhandle to identify the player in future method calls.
    fn add_player(&mut self, player: &Player) -> Result<u32, GGEZError> {
        if player.player_handle > self.num_players {
            return Err(GGEZError::InvalidPlayerHandle);
        }
        Ok(player.player_handle)
    }

    /// After you are done defining and adding all players, you should start the session. In a sync test, starting the session saves the initial game state and sets running to true.
    /// If the session is already running, return an error.
    fn start_session(&mut self) -> Result<(), GGEZError> {
        match self.running {
            true => return Err(GGEZError::InvalidRequest),
            false => self.running = true,
        }

        Ok(())
    }

    /// Used to notify GGEZ of inputs that should be transmitted to remote players. add_local_input must be called once every frame for all players of type [PlayerType::Local].
    /// In the sync test, we don't send anything, we simply save the latest input.
    fn add_local_input(&mut self, player_handle: u32, input: &[u8]) -> Result<(), GGEZError> {
        // player handle is invalid
        if player_handle > self.num_players {
            return Err(GGEZError::InvalidPlayerHandle);
        }
        // session has not been started
        if !self.running {
            return Err(GGEZError::NotSynchronized);
        }
        // copy the local input bits into the right place of the current input
        let lower_bound: usize = player_handle as usize * self.input_size;
        for i in 0..input.len() {
            self.current_input.input_bits[lower_bound + i] |= input[i];
        }
        Ok(())
    }

    /// In a sync test, this will advance the state by a single frame and afterwards rollback "check_distance" amount of frames,
    /// resimulate and compare checksums with the original states. if checksums don't match, this will return [GGEZError::SyncTestFailed].
    fn advance_frame(&mut self, interface: &mut impl GGEZInterface) -> Result<(), GGEZError> {
        // save the current frame in the syncronization layer
        self.sync_layer
            .save_current_state(Some(self.current_input.clone()), interface);

        // save a copy info in our separate queue of saved [ggez::frame_info::FrameInfo] so we have something to compare to later
        //let frame_count = self.sync_layer.get_current_frame();
        match self.sync_layer.get_last_saved_state() {
            Some(fi) => self.saved_frames.push_back(fi.clone()),
            None => return Err(GGEZError::GeneralFailure),
        };

        // advance the frame with the correct inputs (in sync testing that is just the current input)
        interface.advance_frame(&self.current_input, 0);
        self.sync_layer.advance_frame();
        self.frame += 1;

        // current input has been used, so we can delete the input bits
        self.current_input.erase_bits();

        // simulated rollback section, but only if we have enough frames in the queue
        if self.saved_frames.len() > self.check_distance as usize {}

        Ok(())
    }

    /// Nothing happens here in [SyncTestSession]. There are no packets to be received or sent and no rollbacks can occur other than the manually induced ones.
    fn idle(&self, _interface: &mut impl GGEZInterface) -> Result<(), GGEZError> {
        Ok(())
    }

    /// Not supported in [SyncTestSession].
    fn disconnect_player(&mut self, _player_handle: u32) -> Result<(), GGEZError> {
        Err(GGEZError::Unsupported)
    }

    /// Not supported in [SyncTestSession].
    fn get_network_stats(&self, _player_handle: u32) -> Result<NetworkStats, GGEZError> {
        Err(GGEZError::Unsupported)
    }

    /// Not supported in [SyncTestSession].
    fn set_frame_delay(&self, _frame_delay: u32, _player_handle: u32) -> Result<(), GGEZError> {
        Err(GGEZError::Unsupported)
    }

    /// Not supported in [SyncTestSession].
    fn set_disconnect_timeout(&self, _timeout: u32) -> Result<(), GGEZError> {
        Err(GGEZError::Unsupported)
    }

    /// Not supported in [SyncTestSession].
    fn set_disconnect_notify_delay(&self, _notify_delay: u32) -> Result<(), GGEZError> {
        Err(GGEZError::Unsupported)
    }
}

// #########
// # TESTS #
// #########

#[cfg(test)]
mod sync_test_session_tests {
    use adler::Adler32;
    use bincode;
    use serde::{Deserialize, Serialize};
    use std::hash::Hash;

    use crate::frame_info::{GameInput, GameState};
    use crate::player::{Player, PlayerType};
    use crate::{GGEZError, GGEZEvent, GGEZInterface, GGEZSession};

    struct GameStub {
        gs: GameStateStub,
    }

    /*
    impl GameStub {
        fn new() -> GameStub {
            GameStub {
                gs: GameStateStub { frame: 0, state: 0 },
            }
        }
    }
    */

    #[derive(Hash, Default, Serialize, Deserialize)]
    struct GameStateStub {
        pub frame: u32,
        pub state: u32,
    }

    impl GameStateStub {
        fn advance_frame(&mut self, inputs: &GameInput) {
            // we ignore the inputs for now
            let _inputs: u32 = bincode::deserialize(&inputs.input_bits).unwrap();
            self.frame += 1;
            self.state += 2;
        }
    }

    impl GGEZInterface for GameStub {
        fn save_game_state(&self) -> GameState {
            let buffer = bincode::serialize(&self.gs).unwrap();
            let mut adler = Adler32::new();
            self.gs.hash(&mut adler);
            let checksum = adler.checksum();
            GameState {
                buffer,
                checksum: Some(checksum),
            }
        }

        fn load_game_state(&mut self, state: &GameState) {
            self.gs = bincode::deserialize(&state.buffer).unwrap();
        }

        fn advance_frame(&mut self, inputs: &GameInput, _disconnect_flags: u32) {
            self.gs.advance_frame(inputs);
        }

        fn on_event(&mut self, info: GGEZEvent) {
            println!("{:?}", info);
        }
    }

    #[test]
    fn test_add_player() {
        let mut sess = crate::start_synctest_session(1, 2, std::mem::size_of::<u32>());

        // add players correctly
        let dummy_player_0 = Player::new(PlayerType::Local, 0);
        let dummy_player_1 = Player::new(PlayerType::Local, 1);

        match sess.add_player(&dummy_player_0) {
            Ok(handle) => assert_eq!(handle, 0),
            Err(_) => assert!(false),
        }

        match sess.add_player(&dummy_player_1) {
            Ok(handle) => assert_eq!(handle, 1),
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn test_add_player_invalid_handle() {
        let mut sess = crate::start_synctest_session(1, 2, std::mem::size_of::<u32>());

        // add a player incorrectly
        let incorrect_player = Player::new(PlayerType::Local, 3);

        match sess.add_player(&incorrect_player) {
            Err(GGEZError::InvalidPlayerHandle) => (),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_add_local_input_not_running() {
        let mut sess = crate::start_synctest_session(1, 2, std::mem::size_of::<u32>());

        // add 0 input for player 0
        let fake_inputs: u32 = 0;
        let serialized_inputs = bincode::serialize(&fake_inputs).unwrap();

        match sess.add_local_input(0, &serialized_inputs) {
            Err(GGEZError::NotSynchronized) => (),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_add_local_input_invalid_handle() {
        let mut sess = crate::start_synctest_session(1, 2, std::mem::size_of::<u32>());
        sess.start_session().unwrap();

        // add 0 input for player 3
        let fake_inputs: u32 = 0;
        let serialized_inputs = bincode::serialize(&fake_inputs).unwrap();

        match sess.add_local_input(3, &serialized_inputs) {
            Err(GGEZError::InvalidPlayerHandle) => (),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_add_local_input() {
        let mut sess = crate::start_synctest_session(1, 2, std::mem::size_of::<u32>());
        sess.start_session().unwrap();

        // add 0 input for player 0
        let fake_inputs: u32 = 0;
        let serialized_inputs = bincode::serialize(&fake_inputs).unwrap();

        match sess.add_local_input(0, &serialized_inputs) {
            Ok(()) => {
                for i in 0..sess.current_input.input_bits.len() {
                    assert_eq!(sess.current_input.input_bits[i], 0);
                }
            }
            _ => assert!(false),
        }

        // add 1 << 4 input for player 1, now the 5th byte should be 1 << 4
        let fake_inputs: u32 = 1 << 4;
        let serialized_inputs = bincode::serialize(&fake_inputs).unwrap();
        match sess.add_local_input(1, &serialized_inputs) {
            Ok(()) => {
                for i in 0..sess.current_input.input_bits.len() {
                    match i {
                        4 => assert_eq!(sess.current_input.input_bits[i], 16),
                        _ => assert_eq!(sess.current_input.input_bits[i], 0),
                    }
                }
            }
            _ => assert!(false),
        }
    }
}
