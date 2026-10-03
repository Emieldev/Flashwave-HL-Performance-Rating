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
//!
//! It also follows projectiles (Q45, ivg): a rocket or pipe carries how often
//! it has been reflected (`m_iDeflected`), and the moment that counter rises
//! is a Pyro's reflect -- the only trace of one, since the server's own
//! `object_deflected` event never reaches a demo. Each reflect is noted with
//! where it happened, which way the projectile was flying before it, and who
//! sent it back; a reflected projectile's end (exploding, or leaving a POV
//! demo's view) is noted too, which is how a hit is told from a miss.

use std::collections::HashMap;
use tf_demo_parser::demo::data::game_state::PlayerCondition;
use tf_demo_parser::demo::message::packetentities::UpdateType;
use tf_demo_parser::demo::vector::Vector;
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
    /// Projectiles in flight, by entity.
    projectiles: HashMap<EntityId, Flying>,
    /// Every reflect seen, oldest first: the timeline drains these.
    pub reflects: Vec<Reflect>,
    /// Where each reflected projectile ended, oldest first.
    pub reflect_ends: Vec<ReflectEnd>,
}

/// A rocket, pipe or other reflectable projectile, as last seen.
#[derive(Debug, Clone, Default)]
struct Flying {
    what: &'static str,
    origin: Option<[f32; 3]>,
    /// The origin before `origin`, and the tick it was seen: how fast it was
    /// going, for a projectile that does not say.
    before: Option<([f32; 3], u32)>,
    origin_tick: u32,
    /// Rockets fly straight at this; pipes say how they were thrown.
    velocity: Option<[f32; 3]>,
    team: u8,
    owner: Option<u32>,
    deflect_owner: Option<u32>,
    deflected: Option<i64>,
    /// Reflected at least once since entering: its end is worth noting.
    reflected: bool,
    sticky: bool,
}

/// One reflect, as the demo showed it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reflect {
    pub tick: u32,
    /// The projectile's entity: its [`ReflectEnd`] has the same.
    pub entity: u32,
    /// "rocket", "pipe", "sticky", "flare", "arrow", "jar", ...
    pub what: &'static str,
    /// Where it was sent back from.
    pub pos: [f32; 3],
    /// Units a second, the way it was flying before: for whether it would
    /// have hit anyone. `None` when the demo had not shown it long enough.
    pub before: Option<[f32; 3]>,
    /// Its team now, which is the reflector's.
    pub team: u8,
    /// The entity of the player who reflected it, when the projectile says.
    pub by_entity: Option<u32>,
}

/// Where a reflected projectile stopped: it exploded, or (in a POV demo) went
/// out of view, which is `seen: false`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectEnd {
    pub tick: u32,
    pub entity: u32,
    pub pos: [f32; 3],
    pub seen: bool,
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

// Projectiles: rockets (and arrows, flares and the like, which are rockets
// underneath) and grenades (pipes, stickies, jars).
const ROCKET_DEFLECTED: SendPropIdentifier = SendPropIdentifier::new("DT_TFBaseRocket", "m_iDeflected");
const ROCKET_ORIGIN: SendPropIdentifier = SendPropIdentifier::new("DT_TFBaseRocket", "m_vecOrigin");
const ROCKET_VELOCITY: SendPropIdentifier = SendPropIdentifier::new("DT_TFBaseRocket", "m_vInitialVelocity");
const GRENADE_DEFLECTED: SendPropIdentifier = SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_iDeflected");
const GRENADE_ORIGIN: SendPropIdentifier = SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_vecOrigin");
const GRENADE_DEFLECT_OWNER: SendPropIdentifier = SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_hDeflectOwner");
const GRENADE_VELOCITY: SendPropIdentifier = SendPropIdentifier::new("DT_BaseGrenade", "m_vecVelocity");
const PIPE_TYPE: SendPropIdentifier = SendPropIdentifier::new("DT_TFProjectile_Pipebomb", "m_iType");
const ORIGIN: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_vecOrigin");
const OWNER: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_hOwnerEntity");
const TEAM: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_iTeamNum");

/// What a projectile class is, in a word; `None` for anything not
/// reflectable worth counting.
fn projectile_kind(class: &str) -> Option<&'static str> {
    Some(match class {
        "CTFProjectile_Rocket" | "CTFProjectile_SentryRocket" => "rocket",
        "CTFGrenadePipebombProjectile" => "pipe",
        "CTFProjectile_Flare" => "flare",
        "CTFProjectile_Arrow" => "arrow",
        "CTFProjectile_HealingBolt" => "bolt",
        "CTFProjectile_EnergyBall" => "energy",
        "CTFProjectile_Jar" | "CTFProjectile_JarMilk" | "CTFProjectile_JarGas" | "CTFProjectile_Cleaver" => "jar",
        "CTFProjectile_BallOfFire" | "CTFProjectile_SpellFireball" => return None,
        c if c.starts_with("CTFProjectile_") => "projectile",
        _ => return None,
    })
}

/// An entity handle's entity: its low eleven bits.
fn handle_entity(raw: i64) -> Option<u32> {
    let e = (raw & 0x7FF) as u32;
    (raw > 0 && e != 0x7FF).then_some(e)
}

fn vec3(v: Vector) -> [f32; 3] {
    [v.x, v.y, v.z]
}

impl DeepAnalyser {
    /// One projectile's update: note what changed, and a reflect if its
    /// counter rose.
    fn projectile(&mut self, entity: &tf_demo_parser::demo::message::packetentities::PacketEntity, what: &'static str, tick: u32, state: &ParserState) {
        let id = entity.entity_index;
        if matches!(entity.update_type, UpdateType::Enter) {
            // A new projectile, or one back in a POV demo's view: what was
            // known of an earlier one on this entity no longer applies.
            self.projectiles.insert(id, Flying { what, ..Default::default() });
        }
        let f = self.projectiles.entry(id).or_insert_with(|| Flying { what, ..Default::default() });
        let was_deflected = f.deflected;
        let (old_origin, old_velocity, old_before) = (f.origin, f.velocity, f.before);
        let mut deflected = None;
        for prop in entity.props(state) {
            let id = prop.identifier;
            if id == ROCKET_DEFLECTED || id == GRENADE_DEFLECTED {
                deflected = i64::try_from(&prop.value).ok();
            } else if id == ROCKET_ORIGIN || id == GRENADE_ORIGIN || id == ORIGIN {
                if let Ok(v) = Vector::try_from(&prop.value) {
                    if let Some(o) = f.origin.filter(|_| f.origin_tick != tick) {
                        f.before = Some((o, f.origin_tick));
                    }
                    f.origin = Some(vec3(v));
                    f.origin_tick = tick;
                }
            } else if id == ROCKET_VELOCITY || id == GRENADE_VELOCITY {
                if let Ok(v) = Vector::try_from(&prop.value) {
                    f.velocity = Some(vec3(v));
                }
            } else if id == TEAM {
                f.team = i64::try_from(&prop.value).unwrap_or_default() as u8;
            } else if id == OWNER {
                f.owner = i64::try_from(&prop.value).ok().and_then(handle_entity);
            } else if id == GRENADE_DEFLECT_OWNER {
                f.deflect_owner = i64::try_from(&prop.value).ok().and_then(handle_entity);
            } else if id == PIPE_TYPE {
                // 1 is a sticky; 0 a pipe, 3 a cannonball.
                f.sticky = i64::try_from(&prop.value).unwrap_or_default() == 1;
            }
        }
        if let Some(d) = deflected {
            f.deflected = Some(d);
        }
        // A rise seen, not a projectile that came into view already sent
        // back: that one's reflect happened where the demo could not see.
        let rose = matches!((was_deflected, deflected), (Some(a), Some(b)) if b > a);
        if !rose {
            return;
        }
        f.reflected = true;
        // Which way it was going before: a rocket's own velocity, else what
        // its last two positions say.
        let before = if what == "rocket" {
            old_velocity
        } else {
            match (old_origin, old_before) {
                (Some(o), Some((p, pt))) if f.origin_tick > pt => {
                    let dt = (f.origin_tick.saturating_sub(pt)).max(1) as f32 / 66.67;
                    Some([(o[0] - p[0]) / dt, (o[1] - p[1]) / dt, (o[2] - p[2]) / dt])
                }
                _ => None,
            }
        };
        let what = if f.what == "pipe" && f.sticky { "sticky" } else { f.what };
        let reflect = Reflect {
            tick,
            entity: u32::from(id),
            what,
            pos: f.origin.or(old_origin).unwrap_or_default(),
            before,
            team: f.team,
            by_entity: f.deflect_owner.or(f.owner),
        };
        self.reflects.push(reflect);
    }

    /// A projectile is gone: exploded (`seen`), or out of a POV demo's view.
    fn projectile_gone(&mut self, id: EntityId, tick: u32, seen: bool) {
        let Some(f) = self.projectiles.remove(&id) else { return };
        if f.reflected {
            if let Some(pos) = f.origin {
                self.reflect_ends.push(ReflectEnd { tick, entity: u32::from(id), pos, seen });
            }
        }
    }
}

impl MessageHandler for DeepAnalyser {
    type Output = Self;

    fn does_handle(message_type: MessageType) -> bool {
        GameStateAnalyser::does_handle(message_type)
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) {
        self.game.handle_message(message, tick, parser_state);
        let Message::PacketEntities(message) = message else { return };
        let t = u32::from(tick);
        for id in &message.removed_entities {
            self.projectile_gone(*id, t, true);
        }
        for entity in &message.entities {
            let class = self.class_names.get(usize::from(entity.server_class)).map(|n| n.as_str()).unwrap_or("");
            if let Some(what) = projectile_kind(class) {
                match entity.update_type {
                    UpdateType::Delete => self.projectile_gone(entity.entity_index, t, true),
                    // Out of a POV demo's view: how it ended is not known.
                    UpdateType::Leave => self.projectile_gone(entity.entity_index, t, false),
                    _ => self.projectile(entity, what, t, parser_state),
                }
                continue;
            }
            if class != "CTFPlayer" {
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
