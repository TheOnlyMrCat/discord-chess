#[macro_use]
extern crate lazy_static;
extern crate serenity;
extern crate raster;
extern crate chess;
extern crate regex;
extern crate rand;
extern crate png;

use std::collections::{HashSet,HashMap};
use std::sync::{RwLock,Mutex};
use std::borrow::Cow;

use serenity::{
	client::Client,
	model::{
		channel::{
			Message,
			GuildChannel
		},
		gateway::Ready,
		id::{
			ChannelId,
			UserId
		}
	},
	prelude::*,
	framework::standard::{
		StandardFramework,
		CommandResult,
		Args,
		Delimiter,
		HelpOptions,
		CommandGroup,
		macros::{
			command,
			group,
			help
		}
	},
	http::AttachmentType,
};

use rand::prelude::*;
use regex::Regex;
use raster::{Image,PositionMode,BlendMode,editor::ResizeMode};
use chess::{Color,ChessMove,Rank,File,Piece,Square,GameResult};

mod config;
use config::*;

mod game;
use game::*;

lazy_static! {
	static ref CONFIG: Config = Config {
		guild_settings: RwLock::new(HashMap::<_, _>::new()),
		user_prefs: RwLock::new(HashMap::<_, _>::new())
	};

	static ref GAMES: Mutex<HashMap<ChannelId, ChannelGame>> = Mutex::new(HashMap::<_, _>::new());

	static ref BOARD_IMG_WHITE: Image = raster::open("res/board_annotated_white.png").unwrap();
	static ref BOARD_IMG_BLACK: Image = raster::open("res/board_annotated_black.png").unwrap();
	static ref PIECES_IMG: Image = raster::open("res/pieces.png").unwrap();
}

#[group]
#[help_available]
#[only_in(guilds)]
#[commands(play, accept, decline, cancel, preferences)]
struct General;

#[group]
#[help_available]
#[only_in(guilds)]
#[commands(board, draw, resign)]
struct Game;

#[group]
#[help_available]
#[only_in(guilds)]
#[required_permissions(manage_channels)]
#[commands(enable, disable, config, permissions)]
struct Managerial;

#[group]
#[owners_only]
#[prefix = "bot"]
struct Owner;

struct Handler;

impl EventHandler for Handler {
	fn ready(&self, _: Context, _: Ready) {
		println!("Ready");
	}

	fn message(&self, ctx: Context, msg: Message) {
		let mut gs = GAMES.lock().unwrap();

		if let Some(gm) = gs.get_mut(&msg.channel_id) {
			if gm.running && (match gm.game.side_to_move() { Color::White => gm.white, Color::Black => gm.black }) == msg.author.id {
				lazy_static! {
					static ref MOVE_REGEX: Regex = Regex::new(r"^[KQBNR]?[a-h]?[1-8]?x?[a-h][1-8](?:=[BQRN])?[\+#]?( e.p.)?$").unwrap();
					static ref CASTLE_REGEX: Regex = Regex::new("^O-O(-O)?$").unwrap();
				}

				if MOVE_REGEX.is_match(&msg.content) || CASTLE_REGEX.is_match(&msg.content) {
					let result = ChessMove::from_san(&gm.game.current_position(), &msg.content);
					match result {
						Err(chess::Error::InvalidBoard) => { msg.reply(ctx, format!("Ill-formed move: {}", msg.content)).unwrap(); },
						Err(_) => { msg.reply(ctx, format!("Illegal move: {}", msg.content)).unwrap(); }
						Ok(mv) => {
							gm.game.make_move(mv);
							gm.draw_offer = None;
							post_board(&ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read()).unwrap();
						}
					}
				}
			}
		}
	}
}

fn main() {
	use std::io::Write;
	print!("Loading images...");
	std::io::stdout().lock().flush().unwrap();
	lazy_static::initialize(&BOARD_IMG_WHITE);
	lazy_static::initialize(&BOARD_IMG_BLACK);
	lazy_static::initialize(&PIECES_IMG);
	println!(" Done.");

	let mut client = Client::new(std::env::var("DISCORD_TOKEN").unwrap(), Handler).expect("Error creating client");

	let (owners, bot_id) = match client.cache_and_http.http.get_current_application_info() {
		Ok(info) => {
			let mut owners = HashSet::new();
			owners.insert(info.owner.id);

			(owners, info.id)
		},
		Err(why) => panic!("Could not access application info: {:?}", why),
	};

	client.with_framework(StandardFramework::new().configure(|c| c.prefix("c>").on_mention(Some(bot_id)).owners(owners)).group(&GENERAL_GROUP).group(&GAME_GROUP).group(&MANAGERIAL_GROUP).group(&OWNER_GROUP).help(&MAIN_HELP));

	if let Err(reason) = client.start() {
		panic!("An error occured when starting the client: {:?}", reason);
	}
}

fn post_board(ctx: &Context, gm: &mut ChannelGame, ch: &GuildChannel) -> CommandResult {
	CONFIG.lazy_guild(ch.guild_id);
	CONFIG.lazy_user(gm.black);

	ch.broadcast_typing(ctx)?;

	let use_black = gm.game.side_to_move() == Color::Black && CONFIG.user_prefs.read().unwrap().get(&gm.black).unwrap().settings.get("flipIfBlack").unwrap().parse::<bool>().unwrap_or(false);
	let mut board = if use_black { BOARD_IMG_BLACK.clone() } else { BOARD_IMG_WHITE.clone() };

	for y in 0..=7 {
		for x in 0..=7 {
			const RANK_INDEX: [Rank; 8] = [Rank::Eighth, Rank::Seventh, Rank::Sixth, Rank::Fifth, Rank::Fourth, Rank::Third, Rank::Second, Rank::First];
			const FILE_INDEX: [File; 8] = [File::A, File::B, File::C, File::D, File::E, File::F, File::G, File::H];

			const BASE_OFFSET: i32 = 5;
			const PIECE_OFFSET: i32 = 95;
			const WHITE_OFFSET: i32 = 80;

			let square = Square::make_square(RANK_INDEX[y], FILE_INDEX[x]);
			if let Some(piece) = gm.game.current_position().piece_on(square) {
				let mut piece_img = PIECES_IMG.clone();
				raster::editor::crop(&mut piece_img, 55, 55, PositionMode::TopLeft, BASE_OFFSET + PIECE_OFFSET * (match piece {
					Piece::King => 0,
					Piece::Queen => 1,
					Piece::Rook => 2,
					Piece::Bishop => 3,
					Piece::Knight => 4,
					Piece::Pawn => 5
				}), if gm.game.current_position().color_on(square) == Some(Color::White) { WHITE_OFFSET } else { 0 }).unwrap();
				raster::editor::resize(&mut piece_img, 80, 80, ResizeMode::Exact).unwrap();
				let posx: usize;
				let posy: usize;
				if use_black {
					posx = 7 - x;
					posy = 7 - y;
				} else {
					posx = x;
					posy = y;
				}
				board = raster::editor::blend(&board, &piece_img, BlendMode::Normal, 1.0, PositionMode::TopLeft, (40 + posx * 80) as i32, (40 + posy * 80) as i32).unwrap();
			}
		}
	}

	println!("Encoding image");
	let bytes = {
		use png::{Encoder, Compression, ColorType, BitDepth};

		let mut bytes = Vec::<u8>::new();
		let mut encoder = Encoder::new(&mut bytes, board.width as u32, board.height as u32);
		encoder.set_color(ColorType::RGBA);
		encoder.set_depth(BitDepth::Eight);
		encoder.set_compression(Compression::Fast);

		let mut writer = encoder.write_header().unwrap();
		writer.write_image_data(&board.bytes).unwrap();
		std::mem::drop(writer);

		bytes
	};

	println!("Sending board...");
	let sent = ch.send_message(
		ctx,
		|c| {
			c
			.content(format!("{} to play", match gm.game.side_to_move() { Color::White => "White", Color::Black => "Black" }))
			.add_file(AttachmentType::Bytes { data: Cow::from(&bytes), filename: String::from("board.png") });
			if let Some(result) = gm.game.result() {
				c.content(format!("{} to play{}", match gm.game.side_to_move() { Color::White => "White", Color::Black => "Black" },
				match result {
					GameResult::WhiteCheckmates | GameResult::BlackCheckmates => " is checkmated",
					GameResult::WhiteResigns => match gm.game.side_to_move() { Color::White => " has resigned", Color::Black => "; White has resigned" },
					GameResult::BlackResigns => match gm.game.side_to_move() { Color::Black => " has resigned", Color::White => "; Black has resigned" },
					GameResult::Stalemate => " is stalemated",
					GameResult::DrawAccepted => "; Drawn by agreement",
					GameResult::DrawDeclared => "; Draw was declared"
				}));
				gm.running = false;
				gm.finished = true;
			} else {
				if gm.game.can_declare_draw() {
					c.content(format!("{} to play can declare draw", match gm.game.side_to_move() { Color::White => "White", Color::Black => "Black" }));
				}
			}
			c
		}
	)?.id;
	match &**CONFIG.guild_settings.read().unwrap().get(&ch.guild_id).unwrap().settings.get("deleteOld").unwrap() {
		"onNext" => {
			if let Some(b) = gm.old_boards.pop_front() {
				ch.delete_messages(ctx, vec![b])?;
			}
			gm.old_boards.push_back(sent);
		}
		"onEnd" | "onRequest" => {
			if gm.old_boards.len() >= 100 {
				ch.delete_messages(ctx, vec![gm.old_boards.pop_front().unwrap()])?;
			}
			gm.old_boards.push_back(sent);
		}
		"off" => {}
		_ => {
			panic!("Unexpected value for deleteOld");
		}
	}
	println!("Sent");

	Ok(())
}

fn check_perm(msg: &Message, perm: &str) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	match CONFIG.guild_settings.read().unwrap().get(&msg.guild_id.unwrap()).unwrap().get_perm(perm.to_string(), msg.author.id, msg.channel_id) {
		Some((true, _)) => Ok(()),
		_ => Err("Lack of required permissions".into())
	}
}

#[help]
fn main_help(
	ctx: &mut Context,
	msg: &Message,
	args: Args,
	help_options: &'static HelpOptions,
	groups: &[&'static CommandGroup],
	owners: HashSet<UserId>) -> CommandResult
{
	serenity::framework::standard::help_commands::with_embeds(ctx, msg, args, help_options, groups, owners)
}

#[command]
fn play(ctx: &mut Context, msg: &Message) -> CommandResult {
	check_perm(msg, "chess.games.allow")?;
	let mut gs = GAMES.lock().unwrap();

	if let Some(ChannelGame { finished: false, .. }) = gs.get(&msg.channel_id) {
		msg.reply(ctx, "There's already a game going on here!")?;
	} else {
		let mut args = Args::new(&msg.content, &[Delimiter::Single(' ')]);
		args.advance();
		let pla = msg.author.id;
		if let Ok(plb) = args.single::<String>() {
			let idx = if plb.as_bytes()[2] == 33 { 3 } else { 2 };
			let plb = UserId::from(plb[idx..plb.len()-1].parse::<u64>().unwrap());
			let worb = random::<bool>();
			gs.insert(msg.channel_id, ChannelGame {
				white: if worb { pla } else { plb },
				black: if worb { plb } else { pla },
				initiator: if worb { Color::White } else { Color::Black },
				..ChannelGame::new()
			});
			msg.reply(ctx, format!("I've set up your game. You're playing as {}", if worb { "White" } else { "Black" }))?;
		} else {
			msg.reply(ctx, "Who are you playing against? (`c>play @someone`)")?;
		}
	}

	Ok(())
}

#[command]
fn accept(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if let Some(ChannelGame { running: false, finished: false, ..}) = gs.get(&msg.channel_id) {
		let mut gm = gs.get_mut(&msg.channel_id).unwrap();
		if !gm.running {
			if msg.author.id == gm.get_other() {
				gm.running = true;
				post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
			} else {
				msg.reply(ctx, "You weren't asked to play")?;
			}
		} else {
			msg.reply(ctx, "The game has already started")?;
		}
	} else {
		msg.reply(ctx, "No one has started a game in this channel")?;
	}

	Ok(())
}

#[command]
fn decline(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if let Some(ChannelGame { running: false, finished: false, ..}) = gs.get(&msg.channel_id) {
		let gm = gs.get(&msg.channel_id).unwrap();
		if !gm.running {
			if msg.author.id == gm.get_other() {
				msg.channel_id.say(ctx, "The table is now open")?;
				gs.remove(&msg.channel_id);
			} else {
				msg.reply(ctx, "You weren't asked to play")?;
			}
		} else {
			msg.reply(ctx, "The game has already started")?;
		}
	} else {
		msg.reply(ctx, "No one has started a game in this channel")?;
	}

	Ok(())
}

#[command]
fn cancel(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if let Some(ChannelGame { running: false, finished: false, ..}) = gs.get(&msg.channel_id) {
		let gm = gs.get(&msg.channel_id).unwrap();
		if !gm.running {
			if msg.author.id == gm.get_initiator() {
				msg.channel_id.say(ctx, "The table is now open")?;
				gs.remove(&msg.channel_id);
			} else {
				msg.reply(ctx, "You didn't start this game")?;
			}
		} else {
			msg.reply(ctx, "The game has already started")?;
		}
	} else {
		msg.reply(ctx, "No one has started a game in this channel")?;
	}

	Ok(())
}

#[command]
fn board(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if !gs.contains_key(&msg.channel_id) {
		msg.reply(ctx, "No one has started a game in this channel")?;
	} else {
		let mut gm = gs.get_mut(&msg.channel_id).unwrap();
		if gm.running || gm.finished {
			post_board(ctx, &mut gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
		} else {
			msg.reply(ctx, "The game hasn't been accepted yet")?;
		}
	}

	Ok(())
}

#[command]
fn resign(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if !gs.contains_key(&msg.channel_id) {
		msg.reply(ctx, "No one has started a game in this channel")?;
	} else {
		let gm = gs.get_mut(&msg.channel_id).unwrap();
		if gm.running {
			if msg.author.id == gm.white {
				gm.game.resign(Color::White);
				post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
			} else if msg.author.id == gm.black {
				gm.game.resign(Color::Black);
				post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
			} else {
				msg.reply(ctx, "You're not playing this game")?;
			}
		} else {
			msg.reply(ctx, "The game hasn't been accepted yet")?;
		}
	}

	Ok(())
}

#[command]
fn draw(ctx: &mut Context, msg: &Message) -> CommandResult {
	let mut gs = GAMES.lock().unwrap();

	if !gs.contains_key(&msg.channel_id) {
		msg.reply(ctx, "No one has started a game in this channel")?;
	} else {
		let mut gm = gs.get_mut(&msg.channel_id).unwrap();
		if gm.running {
			if msg.author.id == gm.white {
				if gm.game.side_to_move() == Color::White && gm.game.can_declare_draw() {
					gm.game.declare_draw();
					gm.running = false;
					gm.finished = true;
					post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				} else if gm.draw_offer == Some(Color::Black) {
					gm.game.accept_draw();
					gm.running = false;
					gm.finished = true;
					post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				} else {
					gm.draw_offer = Some(Color::White);
					gm.game.offer_draw(Color::White);
					msg.reply(ctx, "You have offered a draw")?;
				}
			} else if msg.author.id == gm.black {
				if gm.game.side_to_move() == Color::Black && gm.game.can_declare_draw() {
					gm.game.declare_draw();
					gm.running = false;
					gm.finished = true;
					post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				} else if gm.draw_offer == Some(Color::White) {
					gm.game.accept_draw();
					gm.running = false;
					gm.finished = true;
					post_board(ctx, gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				} else {
					gm.draw_offer = Some(Color::Black);
					gm.game.offer_draw(Color::White);
					msg.reply(ctx, "You have offered a draw")?;
				}
			} else {
				msg.reply(ctx, "You're not playing this game")?;
			}
		} else {
			msg.reply(ctx, "The game hasn't been accepted yet")?;
		}
	}

	Ok(())
}

#[command]
fn enable(ctx: &mut Context, msg: &Message) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	CONFIG.guild_settings.write().unwrap().get_mut(&msg.guild_id.unwrap()).unwrap().set_perm(format!("games.allow.#{}", msg.channel_id), true);
	msg.reply(ctx, format!("Succesfully enabled all games in channel <#{}>", msg.channel_id))?;

	Ok(())
}

#[command]
fn disable(ctx: &mut Context, msg: &Message) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	CONFIG.guild_settings.write().unwrap().get_mut(&msg.guild_id.unwrap()).unwrap().set_perm(format!("games.allow.#{}", msg.channel_id), false);
	msg.reply(ctx, format!("Succesfully enabled all games in channel <#{}>", msg.channel_id))?;

	Ok(())
}

#[command]
#[aliases("cfg")]
fn config(ctx: &mut Context, msg: &Message) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	let mut lock = CONFIG.guild_settings.write()?;
	let settings = lock.get_mut(&msg.guild_id.unwrap()).unwrap();
	let mut args = Args::new(&msg.content, &[Delimiter::Single(' ')]);
	args.advance();
	let setting = args.single::<String>()?;
	if let Ok(val) = args.single::<String>() {
		let old_value = settings.settings.insert(setting.clone(), val).unwrap_or("".to_string());
		msg.reply(ctx, format!("Value of {} set to \"{}\" (previously \"{}\")", setting, settings.settings.get(&setting).unwrap(), old_value))?;
	} else {
		msg.reply(ctx, format!("Value of {} is: \"{}\"", setting, settings.settings.get(&setting).unwrap_or(&"".to_string())))?;
	}

	Ok(())
}

lazy_static! {
	static ref USER_PING: Regex = Regex::new(r"<@!?(\d+)>").unwrap();
	static ref CHANNEL_REF: Regex = Regex::new(r"<#(\d+)>").unwrap();
}

#[command]
#[aliases("perms")]
fn permissions(ctx: &mut Context, msg: &Message) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	let mut lock = CONFIG.guild_settings.write()?;
	let settings = lock.get_mut(&msg.guild_id.unwrap()).unwrap();
	let mut args = Args::new(&msg.content, &[Delimiter::Single(' ')]);
	args.advance();

	let mode = args.single::<String>()?;
	let mut setting = args.single::<String>()?; //TODO can't have whitespace in setting
	setting.retain(|c| match c {
		'<' | '>' | '!' | ' ' => false,
		_ => true
	});

	if mode == "set" {
		let val = args.single::<bool>()?;
		settings.set_perm(setting.clone(), val);
		msg.reply(ctx, format!("Value of {} set to \"{}\"", setting, val.to_string()))?;
	} else {
		if mode == "reset" {
			settings.unset_perm(setting.clone());
		}
		let opt = settings.get_perm(
			setting.clone(),
			match USER_PING.captures(&setting) {
				Some(caps) => { u64::from_str_radix(caps.get(1).unwrap().as_str(), 10).unwrap().into() }
				None => msg.author.id
			},
			match CHANNEL_REF.captures(&setting) {
				Some(caps) => { u64::from_str_radix(caps.get(1).unwrap().as_str(), 10).unwrap().into() }
				None => msg.channel_id
			});
		match opt {
			Some(perm) => {
				let mut k = setting;
				k.push_str(".@");
				k.push_str(&msg.author.id.to_string());
				k.push_str(".#");
				k.push_str(&msg.channel_id.to_string());
				msg.reply(ctx, format!("Value of {} is: \"{}\"{}", k, perm.0.to_string(), if perm.1 != k { format!(" (Inherited from {})", perm.1) } else { "".to_string() }))?;
			}
			None => {
				msg.reply(ctx, format!("Value of {} is unspecified", setting))?;
			}
		}
	}

	Ok(())
}

#[command]
#[aliases("pref", "prefs")]
fn preferences(ctx: &mut Context, msg: &Message) -> CommandResult {
	check_perm(msg, "prefs.allow")?;
	CONFIG.lazy_user(msg.author.id);
	let mut lock = CONFIG.user_prefs.write()?;
	let prefs = lock.get_mut(&msg.author.id).unwrap();
	let mut args = Args::new(&msg.content, &[Delimiter::Single(' ')]);
	args.advance();
	let setting = args.single::<String>()?; //TODO: what if it isn't?
	if let Ok(val) = args.single::<String>() {
		let old_value = prefs.settings.insert(setting.clone(), val).unwrap_or("".to_string());
		msg.reply(ctx, format!("Value of {} set to \"{}\" (previously \"{}\")", setting, prefs.settings.get(&setting).unwrap(), old_value))?;
	} else {
		msg.reply(ctx, format!("Value of {} is: \"{}\"", setting, prefs.settings.get(&setting).unwrap_or(&"".to_string())))?;
	}

	Ok(())
}