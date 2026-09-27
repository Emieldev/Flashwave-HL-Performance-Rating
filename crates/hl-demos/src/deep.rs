//! The parser's game state, plus the one thing it keeps to itself.
//!
//! `tf-demo-parser` tracks every player's conditions -- scoped, cloaked,
//! ubered, on fire and 120-odd more -- but keeps the bits private, and its
//! public `has_condition` is wrong: it tests `byte >> bit == 1`, which is only
//! true when that condition is the *highest* one set in its byte. A Sniper
//! who is scoped (bit 1) and has just come off a teleporter (bit 6) reads as
//! not scoped; a cloaked Spy (bit 4) who is also being ubered (bit 5) reads as
//! visible. [`DeepAnalyser`] runs the parser's own analyser unchanged and reads
//! the same condition props off the wire alongside it, so [`has`] can answer
//! correctly.

use std::collections::HashMap;
use tf_demo_parser::demo::data::game_state::PlayerCondition;
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::demo::message::packetentities::EntityId;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::packet::datatable::{ParseSendTable, ServerClass, ServerClassName};
use tf_demo_parser::demo::packet::message::MessagePacketMeta;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::gamestateanalyser::{GameState, GameStateAnalyser};
use tf_demo_parser::demo::parser::handler::BorrowMessageHandler;
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::sendprop::SendPropIdentifier;
use tf_demo_parser::{MessageType, ParserState};

/// A player's condition bits, as the server sends them: 20 bytes, bit `n` of
/// the whole being condition `n`.
pub type Conds = [u8; 20];

/// Whether condition `c` is set in `bits`.
pub fn has(bits: &Conds, c: PlayerCondition) -> bool {
    let n = c as usize;
    bits.get(n / 8).is_some_and(|b| (b >> (n % 8)) & 1 == 1)
}

#[derive(Default)]
pub struct DeepAnalyser {
    pub game: GameStateAnalyser,
    /// Condition bits per player entity, kept exactly as the parser would
    /// have kept them had it let anyone read them.
    pub conds: HashMap<EntityId, Conds>,
    class_names: Vec<ServerClassName>,
}

impl DeepAnalyser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &GameState {
        &self.game.state
    }

    /// The condition bits of the player on `entity`; all clear when the demo
    /// has not sent any.
    pub fn conds_of(&self, entity: EntityId) -> Conds {
        self.conds.get(&entity).copied().unwrap_or_default()
    }
}

// The same props the parser reads (gamestateanalyser/player.rs), into the
// same byte ranges.
const COND: SendPropIdentifier = SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCond");
const COND_BITS: SendPropIdentifier = SendPropIdentifier::new("DT_TFPlayerConditionListExclusive", "_condition_bits");
const COND_EX: [SendPropIdentifier; 4] = [
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx"),
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx2"),
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx3"),
    SendPropIdentifier::new("DT_TFPlayerShared", "m_nPlayerCondEx4"),
];

impl MessageHandler for DeepAnalyser {
    type Output = Self;

    fn does_handle(message_type: MessageType) -> bool {
        GameStateAnalyser::does_handle(message_type)
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) {
        self.game.handle_message(message, tick, parser_state);
        let Message::PacketEntities(message) = message else { return };
        for entity in &message.entities {
            let is_player = self
                .class_names
                .get(usize::from(entity.server_class))
                .is_some_and(|name| name.as_str() == "CTFPlayer");
            if !is_player {
                continue;
            }
            for prop in entity.props(parser_state) {
                let range = if prop.identifier == COND || prop.identifier == COND_BITS {
                    0..4
                } else if let Some(i) = COND_EX.iter().position(|id| *id == prop.identifier) {
                    (i + 1) * 4..(i + 2) * 4
                } else {
                    continue;
                };
                let value = i64::try_from(&prop.value).unwrap_or_default();
                let bits = self.conds.entry(entity.entity_index).or_default();
                bits[range].copy_from_slice(&value.to_le_bytes()[0..4]);
            }
        }
    }

    fn handle_string_entry(&mut self, table: &str, index: usize, entry: &StringTableEntry, parser_state: &ParserState) {
        self.game.handle_string_entry(table, index, entry, parser_state);
    }

    fn handle_data_tables(&mut self, tables: &[ParseSendTable], server_classes: &[ServerClass], parser_state: &ParserState) {
        self.class_names = server_classes.iter().map(|c| c.name.clone()).collect();
        self.game.handle_data_tables(tables, server_classes, parser_state);
    }

    fn handle_packet_meta(&mut self, tick: DemoTick, meta: &MessagePacketMeta, parser_state: &ParserState) {
        self.game.handle_packet_meta(tick, meta, parser_state);
    }

    fn into_output(self, _state: &ParserState) -> Self::Output {
        self
    }
}

impl BorrowMessageHandler for DeepAnalyser {
    fn borrow_output(&self, _state: &ParserState) -> &Self::Output {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_condition_does_not_hide_a_lower_one() {
        // Scoped (1) and teleported (6), both in the first byte.
        let mut bits: Conds = [0; 20];
        bits[0] = (1 << 1) | (1 << 6);
        assert!(has(&bits, PlayerCondition::Zoomed));
        assert!(has(&bits, PlayerCondition::Teleported));
        assert!(!has(&bits, PlayerCondition::Stealthed));
    }

    #[test]
    fn conditions_past_the_first_byte_are_found() {
        let mut bits: Conds = [0; 20];
        bits[22 / 8] = 1 << (22 % 8); // Burning
        assert!(has(&bits, PlayerCondition::Burning));
        assert!(!has(&bits, PlayerCondition::Zoomed));
    }
}
