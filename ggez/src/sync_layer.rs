use crate::frame_info::{FrameInfo, GameInput};
use crate::GGEZInterface;
use crate::{circular_buffer::CircularBuffer, GGEZError};

#[derive(Debug, Default)]
pub struct SyncLayer {
    num_players: u32,
    input_size: usize,
    saved_frames: CircularBuffer<FrameInfo>,
    rolling_back: bool,
    last_confirmed_frame: i32,
    frame: u32,
}

impl SyncLayer {
    /// Creates a new [SyncLayer] instance with given values.
    pub fn new(num_players: u32, input_size: usize) -> Self {
        SyncLayer {
            num_players,
            input_size,
            rolling_back: false,
            last_confirmed_frame: -1,
            frame: 0,
            saved_frames: CircularBuffer::new(crate::MAX_PREDICTION_FRAMES as usize),
        }
    }

    pub fn get_current_frame(&self) -> u32 {
        self.frame
    }

    pub fn advance_frame(&mut self) {
        self.frame += 1;
    }

    pub fn save_current_state(&mut self, input: Option<GameInput>, interface: &impl GGEZInterface) {
        let input_to_save: GameInput;
        match input {
            Some(inp) => input_to_save = inp,
            None => {
                input_to_save = GameInput::new(self.input_size * self.num_players as usize, None)
            }
        }
        self.saved_frames.push_back(FrameInfo {
            frame: self.frame,
            state: interface.save_game_state(),
            input: input_to_save,
        });
    }

    pub fn get_last_saved_state(&self) -> Option<&FrameInfo> {
        self.saved_frames.front()
    }

    /// Loads the gamestate indicated by the frame_to_load. After execution, the desired frame is on the back of the gamestate queue.
    /// TODO: If you specify a frame_to_load which does not exist, the sync_layer will be emptied and the whole session is unrecoverably ruined.
    pub fn load_frame(
        &mut self,
        interface: &mut impl GGEZInterface,
        frame_to_load: u32,
    ) -> Result<(), GGEZError> {
        // The state is the current state (not yet saved) or the state cannot possibly be inside our queue since it is too far away in the past
        if self.frame == frame_to_load
            || frame_to_load > self.frame
            || frame_to_load < self.frame - crate::MAX_PREDICTION_FRAMES
        {
            return Err(GGEZError::InvalidRequest);
        }
        let pos = self.frame - frame_to_load;
        let frame_info = self
            .saved_frames
            .get(pos as usize)
            .ok_or(GGEZError::GeneralFailure)?;

        assert_eq!(frame_info.frame, frame_to_load);
        interface.load_game_state(&frame_info.state);

        Ok(())
    }
}
