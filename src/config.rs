use serenity::model::id::{GuildId,UserId,ChannelId};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct Config {
	pub guild_settings: Mutex<HashMap<GuildId, GuildConfig>>,
	pub user_prefs: Mutex<HashMap<UserId, UserConfig>>
}

pub struct GuildConfig {
	pub settings: HashMap<String, String>,
	pub permissions: HashMap<String, bool>,
}

pub struct UserConfig {
	pub settings: HashMap<String, String>,
}

impl Config {
	#[inline]
	pub fn lazy_guild(&self, id: GuildId) {
		self.guild_settings.lock().unwrap().entry(id).or_insert(GuildConfig::new());
	}

	#[inline]
	pub fn lazy_user(&self, id: UserId) {
		self.user_prefs.lock().unwrap().entry(id).or_insert(UserConfig::new());
	}
}

impl GuildConfig {
	fn new() -> GuildConfig {
		let mut gc = GuildConfig { settings: HashMap::new(), permissions: HashMap::new() };
		gc.settings.insert("deleteOld".to_string(), "onNext".to_string());
		gc.permissions.insert("games.allow".to_string(), false);
		gc.permissions.insert("allow".to_string(), true);
		gc
	}

	pub fn get_perm(&self, key: String, user: UserId, channel: ChannelId) -> Option<(bool, String)> {
		let keys: Vec<&str> = key.split('.').collect();
		for i in 0..keys.len() {
			let base: String = keys[i..keys.len()].iter().flat_map(|s| s.chars().chain(".".chars())).collect();
			let base = String::from(&base[0..base.len()-1]);
			let u = &user.to_string();
			let c = &channel.to_string();
			let mut k = base.clone();
			k.push_str(".@");
			k.push_str(u);
			k.push_str(".#");
			k.push_str(c);
			if let Some(b) = self.permissions.get(&k) {
				return Some((*b, k));
			}
			k = base.clone();
			k.push_str(".@");
			k.push_str(u);
			let o_u = self.permissions.get(&k);
			k = base.clone();
			k.push_str(".#");
			k.push_str(c);
			let o_c = self.permissions.get(&k);
			if let Some(b_u) = o_u {
				return Some((*b_u && match o_c { Some(b) => *b, None => true }, k));
			} else if let Some(b_c) = o_c {
				return Some((*b_c, k));
			}
			k = base.clone();
			if let Some(b) = self.permissions.get(&k) {
				return Some((*b, k));
			}
		}
		None
	}

	#[inline]
	pub fn set_perm(&mut self, key: String, value: bool) {
		self.permissions.insert(key, value);
	}

	#[inline]
	pub fn unset_perm(&mut self, key: String) {
		self.permissions.remove(&key);
	}
}

impl UserConfig {
	fn new() -> UserConfig {
		let cfg = UserConfig {
			settings: HashMap::new()
		};
		// cfg.settings.insert("flipIfBlack", "true");
		cfg
	}
}