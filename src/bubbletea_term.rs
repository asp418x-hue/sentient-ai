use bubbletea::{self, Message, Model, Program, Cmd, KeyMsg, KeyType};

struct SentientDashboard {
    cbt_stage: String,
    temperature: f32,
    active_subagents: usize,
}

impl Model for SentientDashboard {
    fn init(&self) -> Option<Cmd> {
        None
    }

    fn update(&mut self, msg: Message) -> Option<Cmd> {
        // Handle keyboard inputs
        if let Some(key) = msg.downcast_ref::<KeyMsg>() {
            match key.key_type {
                KeyType::Char(c) if c == 'q' => return Some(bubbletea::quit()),
                KeyType::CtrlC => return Some(bubbletea::quit()),
                _ => {}
            }
        }
        None
    }

    fn view(&self) -> String {
        format!(
            "=========================================\n\
             [SENTIENT-AI] Local Sovereign Dashboard  \n\
             =========================================\n\
             Current CBT Stage : {}\n\
             System Temp       : {:.1}°C\n\
             Active Subagents  : {}\n\
             -----------------------------------------\n\
             Press 'q' to exit.\n",
            self.cbt_stage, self.temperature, self.active_subagents
        )
    }
}

fn main() -> Result<(), bubbletea::Error> {
    let dashboard = SentientDashboard {
        cbt_stage: "Stage 1: Examine".into(),
        temperature: 38.1,
        active_subagents: 8,
    };

    let _final_model = Program::new(dashboard)
        .with_alt_screen()
        .run()?;

    Ok(())
}

