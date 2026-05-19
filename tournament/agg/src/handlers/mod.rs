//! Tournament aggregate command handlers.

mod chip_economy;
mod create;
mod day_end;
mod enroll;
mod hand_for_hand;
mod lifecycle;
mod penalty;
mod player_lifecycle;
mod rebuy;
mod registration;

pub use chip_economy::{handle_color_up, handle_rebalance_tables};
pub use create::handle_create_tournament;
pub use day_end::{handle_bag_and_tag, handle_rotate_mixed_game_variant, handle_stop_new_hands};
pub use enroll::handle_enroll_player;
pub use hand_for_hand::{
    handle_enter_hand_for_hand, handle_record_h4h_hand, handle_record_round_complete,
    handle_record_table_hand_complete,
};
pub use lifecycle::{
    handle_advance_blind_level, handle_complete_tournament, handle_eliminate_player,
    handle_pause_tournament, handle_resume_tournament, handle_start_tournament,
};
pub use penalty::{handle_decrement_penalty, handle_disqualify_player, handle_issue_penalty};
pub use player_lifecycle::{
    handle_advance_absent_blind, handle_award_bounty, handle_detect_no_show,
    handle_re_entry_player, handle_record_simultaneous_busts, handle_reseat_absent_player,
    handle_trigger_seat_redraw,
};
pub use rebuy::handle_process_rebuy;
pub use registration::{handle_close_registration, handle_open_registration};
