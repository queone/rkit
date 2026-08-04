//! Draw/prize data types and JSON (de)serialization for `cash5`, ported
//! from Go's `model.go`.
//!
//! Manual `serde_json::Value` mapping is used instead of `#[derive]`
//! (matching `jy.rs`'s and `web.rs`'s hand-bridge convention) since adding
//! `serde`'s derive feature as a direct dependency is outside this AC's
//! "no new dependency" scope. Every key is always written on save — Go's
//! `omitempty` tags only affect whether a zero-valued field is *present* in
//! the JSON, and an absent key round-trips to the same zero value as a
//! present-but-zero one, so omitting `omitempty` fidelity does not change
//! parsed results for any file this program writes or reads.

use serde_json::{Map, Value};

#[derive(Debug, Clone, Default)]
pub struct Draw {
    pub game_name: String,
    pub id: String,
    pub status: String,
    pub draw_time: i64,
    pub estimated_jackpot: i64,
    pub jackpot: i64,
    pub actual_payout: i64,
    pub results: Vec<DrawResult>,
    pub prize_tiers: Vec<PrizeTier>,
    pub prizes: Vec<Prize>,
    pub winning_numbers: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct DrawResult {
    pub primary: Vec<String>,
    pub primary_reveal_order: Vec<String>,
    pub draw_type: String,
    pub winners: i64,
    pub payout: i64,
    pub prize_amount: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PrizeTier {
    pub tier: String,
    pub winners: i64,
    pub prize_amount: i64,
    pub description: String,
    pub match_tier: String,
    pub prize: i64,
    pub share_count: i64,
    pub share_amount: i64,
    pub name: String,
    pub id: String,
    pub prize_type: String,
    pub draw_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct Prize {
    pub level: String,
    pub winners: i64,
    pub amount: i64,
    pub description: String,
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn i64_at(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn str_vec_at(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

impl PrizeTier {
    fn from_value(v: &Value) -> Self {
        Self {
            tier: str_at(v, "tier"),
            winners: i64_at(v, "winners"),
            prize_amount: i64_at(v, "prizeAmount"),
            description: str_at(v, "description"),
            match_tier: str_at(v, "match"),
            prize: i64_at(v, "prize"),
            share_count: i64_at(v, "shareCount"),
            share_amount: i64_at(v, "shareAmount"),
            name: str_at(v, "name"),
            id: str_at(v, "id"),
            prize_type: str_at(v, "prizeType"),
            draw_type: str_at(v, "drawType"),
        }
    }

    /// True for a tier the Go original would drop as an "empty" prize tier
    /// row (its custom `UnmarshalJSON` filter).
    fn is_empty(&self) -> bool {
        self.tier.is_empty()
            && self.winners <= 0
            && self.prize_amount <= 0
            && self.name.is_empty()
            && self.share_count <= 0
            && self.share_amount <= 0
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("tier".into(), Value::String(self.tier.clone()));
        map.insert("winners".into(), Value::from(self.winners));
        map.insert("prizeAmount".into(), Value::from(self.prize_amount));
        map.insert(
            "description".into(),
            Value::String(self.description.clone()),
        );
        map.insert("match".into(), Value::String(self.match_tier.clone()));
        map.insert("prize".into(), Value::from(self.prize));
        map.insert("shareCount".into(), Value::from(self.share_count));
        map.insert("shareAmount".into(), Value::from(self.share_amount));
        map.insert("name".into(), Value::String(self.name.clone()));
        map.insert("id".into(), Value::String(self.id.clone()));
        map.insert("prizeType".into(), Value::String(self.prize_type.clone()));
        map.insert("drawType".into(), Value::String(self.draw_type.clone()));
        Value::Object(map)
    }
}

impl Prize {
    fn from_value(v: &Value) -> Self {
        Self {
            level: str_at(v, "level"),
            winners: i64_at(v, "winners"),
            amount: i64_at(v, "amount"),
            description: str_at(v, "description"),
        }
    }

    /// True for a prize the Go original would drop as an "empty" prize row.
    fn is_empty(&self) -> bool {
        self.level.is_empty() && self.winners <= 0 && self.amount <= 0
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("level".into(), Value::String(self.level.clone()));
        map.insert("winners".into(), Value::from(self.winners));
        map.insert("amount".into(), Value::from(self.amount));
        map.insert(
            "description".into(),
            Value::String(self.description.clone()),
        );
        Value::Object(map)
    }
}

impl DrawResult {
    fn from_value(v: &Value) -> Self {
        Self {
            primary: str_vec_at(v, "primary"),
            primary_reveal_order: str_vec_at(v, "primaryRevealOrder"),
            draw_type: str_at(v, "drawType"),
            winners: i64_at(v, "winners"),
            payout: i64_at(v, "payout"),
            prize_amount: i64_at(v, "prizeAmount"),
        }
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "primary".into(),
            Value::Array(self.primary.iter().cloned().map(Value::String).collect()),
        );
        map.insert(
            "primaryRevealOrder".into(),
            Value::Array(
                self.primary_reveal_order
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        map.insert("drawType".into(), Value::String(self.draw_type.clone()));
        map.insert("winners".into(), Value::from(self.winners));
        map.insert("payout".into(), Value::from(self.payout));
        map.insert("prizeAmount".into(), Value::from(self.prize_amount));
        Value::Object(map)
    }
}

impl Draw {
    /// Builds a `Draw` from a JSON value, applying the same empty-tier and
    /// empty-prize filter as Go's `Draw.UnmarshalJSON`.
    pub fn from_value(v: &Value) -> Self {
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(DrawResult::from_value).collect())
            .unwrap_or_default();
        let prize_tiers = v
            .get("prizeTiers")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(PrizeTier::from_value)
                    .filter(|tier| !tier.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let prizes = v
            .get("prizes")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(Prize::from_value)
                    .filter(|prize| !prize.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Self {
            game_name: str_at(v, "gameName"),
            id: str_at(v, "id"),
            status: str_at(v, "status"),
            draw_time: i64_at(v, "drawTime"),
            estimated_jackpot: i64_at(v, "estimatedJackpot"),
            jackpot: i64_at(v, "jackpot"),
            actual_payout: i64_at(v, "actualPayout"),
            results,
            prize_tiers,
            prizes,
            winning_numbers: v.get("winningNumbers").cloned(),
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("gameName".into(), Value::String(self.game_name.clone()));
        map.insert("id".into(), Value::String(self.id.clone()));
        map.insert("status".into(), Value::String(self.status.clone()));
        map.insert("drawTime".into(), Value::from(self.draw_time));
        map.insert(
            "estimatedJackpot".into(),
            Value::from(self.estimated_jackpot),
        );
        map.insert("jackpot".into(), Value::from(self.jackpot));
        map.insert("actualPayout".into(), Value::from(self.actual_payout));
        map.insert(
            "results".into(),
            Value::Array(self.results.iter().map(DrawResult::to_value).collect()),
        );
        map.insert(
            "prizeTiers".into(),
            Value::Array(self.prize_tiers.iter().map(PrizeTier::to_value).collect()),
        );
        map.insert(
            "prizes".into(),
            Value::Array(self.prizes.iter().map(Prize::to_value).collect()),
        );
        map.insert(
            "winningNumbers".into(),
            self.winning_numbers.clone().unwrap_or(Value::Null),
        );
        Value::Object(map)
    }
}

/// Parses a `{"draws": [...]}` API response body into `Draw`s.
pub fn parse_api_response(body: &[u8]) -> Result<Vec<Draw>, String> {
    let root: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    Ok(root
        .get("draws")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(Draw::from_value).collect())
        .unwrap_or_default())
}

/// Parses a bare JSON array of `Draw`s — the local state-file format.
pub fn parse_draws_array(text: &str) -> Result<Vec<Draw>, String> {
    let root: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let items = root
        .as_array()
        .ok_or_else(|| "expected a JSON array of draws".to_owned())?;
    Ok(items.iter().map(Draw::from_value).collect())
}

/// Encodes `draws` as a 2-space-indented JSON array, matching Go's
/// `json.Encoder.SetIndent("", "  ")`.
pub fn encode_draws(draws: &[Draw]) -> String {
    let array = Value::Array(draws.iter().map(Draw::to_value).collect());
    serde_json::to_string_pretty(&array).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_empty_prize_tiers_and_prizes() {
        let raw = serde_json::json!({
            "gameName": "Cash 5",
            "id": "d1",
            "drawTime": 1_735_689_600_000i64,
            "results": [{"primary": ["1", "2", "3", "4", "5"]}],
            "prizeTiers": [
                {"tier": "1", "winners": 1, "prizeAmount": 50000},
                {}
            ],
            "prizes": [
                {"level": "5/5", "winners": 1, "amount": 50000},
                {}
            ]
        });
        let draw = Draw::from_value(&raw);
        assert_eq!(draw.prize_tiers.len(), 1);
        assert_eq!(draw.prizes.len(), 1);
        assert_eq!(draw.results[0].primary, vec!["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn round_trips_through_encode_and_parse() {
        let draws = vec![Draw {
            game_name: "Cash 5".to_owned(),
            id: "d1".to_owned(),
            draw_time: 1_735_689_600_000,
            results: vec![DrawResult {
                primary: vec!["1".into(), "2".into(), "3".into(), "4".into(), "5".into()],
                ..Default::default()
            }],
            ..Default::default()
        }];
        let encoded = encode_draws(&draws);
        let parsed = parse_draws_array(&encoded).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "d1");
        assert_eq!(parsed[0].draw_time, 1_735_689_600_000);
        assert_eq!(parsed[0].results[0].primary, draws[0].results[0].primary);
    }

    #[test]
    fn parse_api_response_reads_draws_array() {
        let body = br#"{"draws":[{"id":"d1","gameName":"Cash 5"}]}"#;
        let draws = parse_api_response(body).unwrap();
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].id, "d1");
    }
}
