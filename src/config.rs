use serenity::model::id::{GuildId,UserId,ChannelId};
use std::collections::HashMap;
use std::sync::RwLock;

pub struct Config {
	pub guild_settings: RwLock<HashMap<GuildId, GuildConfig>>,
	pub user_prefs: RwLock<HashMap<UserId, UserConfig>>
}

pub struct GuildConfig {
	pub settings: HashMap<String, String>,
	pub permissions: HashMap<String, bool>,
}

pub struct UserConfig {
	pub settings: HashMap<String, String>,
}

impl Config {
	pub fn lazy_guild(&self, id: GuildId) {
		self.guild_settings.write().unwrap().entry(id).or_insert_with(GuildConfig::new);
	}

	pub fn lazy_user(&self, id: UserId) {
		self.user_prefs.write().unwrap().entry(id).or_insert_with(UserConfig::new);
	}
}

impl GuildConfig {
	fn new() -> GuildConfig {
		let mut gc = GuildConfig { settings: HashMap::new(), permissions: HashMap::new() };
		gc.settings.insert("deleteOld".to_string(), "onNext".to_string());
		gc.permissions.insert("allow".to_string(), true);
		gc
	}

	pub fn get_perm(&self, key: String, user: UserId, channel: ChannelId) -> Option<(bool, String)> {
		let keys: Vec<&str> = key.split('.').collect();
		for i in 0..keys.len() {
			let base: String = keys[i..keys.len()].iter().flat_map(|s| s.chars().chain(".".chars())).collect();
			let base = String::from(&base[0..base.len()-1]); // Cut off last .
			let u = &user.to_string();
			let c = &channel.to_string();

			// User in channel
			{
				let mut k = base.clone();
				k.push_str(".@");
				k.push_str(u);
				k.push_str(".#");
				k.push_str(c);
				if let Some(b) = self.permissions.get(&k) {
					return Some((*b, k));
				}
			}

			// User in guild
			let o_u = {	
				let mut k = base.clone();
				k.push_str(".@");
				k.push_str(u);
				(self.permissions.get(&k), k)
			};

			// Channel in guild
			let o_c = {
				let mut k = base.clone();
				k.push_str(".#");
				k.push_str(c);
				(self.permissions.get(&k), k)
			};

			if let (Some(b_u), k) = o_u {
				return Some((*b_u && match o_c { (Some(b), _) => *b, (None, _) => true }, k));
			} else if let (Some(b_c), k) = o_c {
				return Some((*b_c, k));
			}

			let k = base.clone();
			if let Some(b) = self.permissions.get(&k) {
				return Some((*b, k));
			}
		}
		None
	}

	pub fn set_perm(&mut self, key: String, value: bool) {
		self.permissions.insert(key, value);
	}

	pub fn unset_perm(&mut self, key: String) {
		self.permissions.remove(&key);
	}
}

impl UserConfig {
	fn new() -> UserConfig {
		let mut cfg = UserConfig {
			settings: HashMap::new()
		};
		cfg.settings.insert("flipIfBlack".to_string(), "true".to_string());
		cfg
	}
}