#[macro_use] extern crate lazy_static;

use std::collections::{HashSet,HashMap};
use std::sync::{RwLock};
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
use raster::{Image,PositionMode,BlendMode};
use chess::{Color,ChessMove,Rank,File,Piece,Square,GameResult};

use chashmap::CHashMap;

mod config;
use config::*;

mod game;
use game::*;

//MARK: Statics
lazy_static! {
	static ref CONFIG: Config = Config {
		guild_settings: RwLock::new(HashMap::<_, _>::new()),
		user_prefs: RwLock::new(HashMap::<_, _>::new())
	};

	static ref GAMES: CHashMap<ChannelId, ChannelGame> = CHashMap::<_, _>::new();
	static ref USERS: CHashMap<UserId, UserStats> = CHashMap::<_, _>::new();

	static ref BOARD_IMG_WHITE: Image = raster::open("res/board_annotated_white.png").unwrap();
	static ref BOARD_IMG_BLACK: Image = raster::open("res/board_annotated_black.png").unwrap();

	static ref PAWNS_IMG: Image = raster::open("res/pawns.png").unwrap();
	static ref KNIGHTS_IMG: Image = raster::open("res/knights.png").unwrap();
	static ref BISHOPS_IMG: Image = raster::open("res/bishops.png").unwrap();
	static ref ROOKS_IMG: Image = raster::open("res/rooks.png").unwrap();
	static ref QUEENS_IMG: Image = raster::open("res/queens.png").unwrap();
	static ref KINGS_IMG: Image = raster::open("res/kings.png").unwrap();

	static ref YELLOW_SQUARE: Image = {
		let mut img = raster::Image::blank(80, 80);
		raster::editor::fill(&mut img, raster::Color::rgb(255, 255, 127)).unwrap();
		img
	};
	static ref RED_SQUARE: Image = {
		let mut img = raster::Image::blank(80, 80);
		raster::editor::fill(&mut img, raster::Color::rgb(255, 83, 83)).unwrap();
		img
	};
}

#[group]
#[help_available]
#[only_in(guilds)]
#[commands(play, accept, decline, cancel, preferences, statistics)]
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

	//MARK: Message handler
	fn message(&self, ctx: Context, msg: Message) {
		if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {
			if gm.state == ChannelGameState::Running && (match gm.game.side_to_move() { Color::White => gm.white, Color::Black => gm.black }) == msg.author.id {
				lazy_static! {
					static ref MOVE_REGEX: Regex = Regex::new(r"^[KQBNR]?[a-h]?[1-8]?x?[a-h][1-8](?:=[BQRN])?[\+#]?( e.p.)?$").unwrap();
					static ref CASTLE_REGEX: Regex = Regex::new("^O-O(-O)?$").unwrap();
				}

				if MOVE_REGEX.is_match(&msg.content) || CASTLE_REGEX.is_match(&msg.content) {
					let result = ChessMove::from_san(&gm.game.current_position(), &msg.content);
					match result {
						Err(game::MoveError::IllFormed) => { msg.reply(ctx, format!("Ill-formed move: {}", msg.content)).unwrap(); },
						Err(game::MoveError::Illegal) => { msg.reply(ctx, format!("Illegal move: {}", msg.content)).unwrap(); }
						Err(game::MoveError::Ambiguous) => { msg.reply(ctx, format!("Ambiguous move: {}", msg.content)).unwrap(); }
						Ok(mv) => {
							gm.game.make_move(mv);
							gm.last_move = Some(mv);
							gm.draw_offer = None;
							post_board(&ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read()).unwrap();
							
							USERS.alter(msg.author.id, |opt| match opt {
								Some(stats) => Some(stats),
								None=> Some(UserStats::default())
							});
							let mut author_stats = USERS.get_mut(&msg.author.id).unwrap();
							author_stats.moves_made += 1;
							if msg.content.contains('x') { // All capture moves must contain 'x', lest they be marked illegal
								author_stats.pieces_captured += 1;
							}
							if gm.game.current_position().checkers().popcnt() > 0 { // This move put the opponent's king in check
								author_stats.checks_given += 1;
							}
							std::mem::drop(author_stats);

							check_game_result(&mut gm);
						}
					}
				}
			}
		}
	}
}

//MARK: Main
fn main() {
	use std::io::Write;
	print!("Loading images...");
	std::io::stdout().lock().flush().unwrap();

	lazy_static::initialize(&BOARD_IMG_WHITE);
	lazy_static::initialize(&BOARD_IMG_BLACK);
	lazy_static::initialize(&PAWNS_IMG);
	lazy_static::initialize(&KNIGHTS_IMG);
	lazy_static::initialize(&BISHOPS_IMG);
	lazy_static::initialize(&ROOKS_IMG);
	lazy_static::initialize(&QUEENS_IMG);
	lazy_static::initialize(&KINGS_IMG);

	lazy_static::initialize(&YELLOW_SQUARE);
	lazy_static::initialize(&RED_SQUARE);

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

//MARK: Board
fn post_board(ctx: &Context, gm: &ChannelGame, ch: &GuildChannel) -> CommandResult {
	CONFIG.lazy_guild(ch.guild_id);
	CONFIG.lazy_user(gm.black);

	ch.broadcast_typing(ctx)?;

	let use_black = gm.game.side_to_move() == Color::Black && CONFIG.user_prefs.read().unwrap().get(&gm.black).unwrap().settings.get("flipIfBlack").unwrap().parse::<bool>().unwrap_or(false);
	let mut board = if use_black { BOARD_IMG_BLACK.clone() } else { BOARD_IMG_WHITE.clone() };

	const RANK_INDEX: [Rank; 8] = [Rank::Eighth, Rank::Seventh, Rank::Sixth, Rank::Fifth, Rank::Fourth, Rank::Third, Rank::Second, Rank::First];
	const FILE_INDEX: [File; 8] = [File::A, File::B, File::C, File::D, File::E, File::F, File::G, File::H];

	for (y, &rank) in RANK_INDEX.iter().enumerate() {
		for (x, &file) in FILE_INDEX.iter().enumerate() {
			let posx: usize;
			let posy: usize;
			if use_black {
				posx = 7 - x;
				posy = 7 - y;
			} else {
				posx = x;
				posy = y;
			}

			let square = Square::make_square(rank, file);
			if let Some(last_move) = gm.last_move {
				if square == last_move.get_source() || square == last_move.get_dest() {
					board = raster::editor::blend(&board, &YELLOW_SQUARE, BlendMode::Normal, 1.0, PositionMode::TopLeft, (40 + posx * 80) as i32, (40 + posy * 80) as i32).unwrap();
				}
			}

			if gm.game.current_position().checkers().popcnt() > 0
				&& square == gm.game.current_position().king_square(gm.game.side_to_move()) {
				board = raster::editor::blend(&board, &RED_SQUARE, BlendMode::Normal, 1.0, PositionMode::TopLeft, (40 + posx * 80) as i32, (40 + posy * 80) as i32).unwrap();
			}
			
			if let Some(piece) = gm.game.current_position().piece_on(square) {
				let (mut piece_img, white_ctr, black_ctr) = match piece {
					Piece::Pawn => (PAWNS_IMG.clone(), 46, 116),
					Piece::Knight => (KNIGHTS_IMG.clone(), 45, 120),
					Piece::Bishop => (BISHOPS_IMG.clone(), 43, 118),
					Piece::Rook => (ROOKS_IMG.clone(), 45, 116),
					Piece::Queen => (QUEENS_IMG.clone(), 41, 120),
					Piece::King => (KINGS_IMG.clone(), 42, 118),
				};
				raster::editor::crop(
					&mut piece_img,
					80, 80, PositionMode::TopLeft,
					if gm.game.current_position().color_on(square) == Some(Color::White) {
						white_ctr - 40
					} else {
						black_ctr - 40
					},
					0
				).unwrap();
				// raster::editor::resize(&mut piece_img, 80, 80, ResizeMode::Exact).unwrap();
				board = raster::editor::blend(&board, &piece_img, BlendMode::Normal, 1.0, PositionMode::TopLeft, (40 + posx * 80) as i32, (40 + posy * 80) as i32).unwrap();
			}
		}
	}

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
			} else if gm.game.can_declare_draw() {
				c.content(format!("{} to play can declare draw", match gm.game.side_to_move() { Color::White => "White", Color::Black => "Black" }));
			}
			c
		}
	)?.id;
	match &**CONFIG.guild_settings.read()?.get(&ch.guild_id).unwrap().settings.get("deleteOld").unwrap() {
		"onNext" => {
			let mut lock = gm.old_boards.lock()?;
			if let Some(b) = lock.pop_front() {
				ch.delete_messages(ctx, vec![b])?;
			}
			lock.push_back(sent);
		}
		"onEnd" | "onRequest" => {
			let mut lock = gm.old_boards.lock()?;
			if lock.len() >= 100 {
				ch.delete_messages(ctx, vec![lock.pop_front().unwrap()])?;
			}
			lock.push_back(sent);
		}
		_ => {} // Includes "off"
	}

	Ok(())
}

fn check_game_result(gm: &mut ChannelGame) {
	if let Some(result) = gm.game.result() {
		USERS.alter(gm.white, |opt| match opt {
			Some(stats) => Some(stats),
			None=> Some(UserStats::default())
		});
		USERS.alter(gm.black, |opt| match opt {
			Some(stats) => Some(stats),
			None=> Some(UserStats::default())
		});
		let mut white_stats = USERS.get_mut(&gm.white).unwrap();
		let mut black_stats = USERS.get_mut(&gm.black).unwrap();
		match result {
			GameResult::WhiteCheckmates => {
				white_stats.won_checkmate += 1;
				black_stats.lost_checkmate += 1;
			},
			GameResult::BlackResigns => {
				white_stats.won_default += 1;
				black_stats.lost_resigned += 1;
			},
			GameResult::BlackCheckmates => {
				white_stats.lost_checkmate += 1;
				black_stats.won_checkmate += 1;
			},
			GameResult::WhiteResigns => {
				white_stats.lost_resigned += 1;
				black_stats.won_default += 1;
			},
			GameResult::Stalemate => {
				white_stats.drawn_stalemate += 1;
				black_stats.drawn_stalemate += 1;
			},
			GameResult::DrawAccepted => {
				white_stats.drawn_agreement += 1;
				black_stats.drawn_agreement += 1;
			},
			GameResult::DrawDeclared => {
				white_stats.drawn_declared += 1;
				black_stats.drawn_declared += 1;
			},
		}

		gm.state = ChannelGameState::Inactive;
	}
}

fn check_perm(msg: &Message, perm: &str) -> CommandResult {
	CONFIG.lazy_guild(msg.guild_id.unwrap());
	match CONFIG.guild_settings.read().unwrap().get(&msg.guild_id.unwrap()).unwrap().get_perm(perm.to_string(), msg.author.id, msg.channel_id) {
		Some((true, _)) => Ok(()),
		_ => Err("Lack of required permissions".into())
	}
}

//MARK: Commands
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
	GAMES.alter(msg.channel_id, |opt| match opt {
		Some(gm) => Some(gm),
		None => Some(ChannelGame::new()),
	});
	let mut gm = GAMES.get_mut(&msg.channel_id).unwrap();

	if gm.state != ChannelGameState::Inactive {
		msg.reply(ctx, "There's already a game going on here!")?;
	} else {
		let mut args = Args::new(&msg.content, &[Delimiter::Single(' ')]);
		args.advance();
		let pla = msg.author.id;
		if let Ok(plb) = args.single::<String>() {
			let idx = if plb.as_bytes()[2] == 33 { 3 } else { 2 };
			let plb = UserId::from(plb[idx..plb.len()-1].parse::<u64>()?);
			let worb = random::<bool>();
			*gm = ChannelGame {
				white: if worb { pla } else { plb },
				black: if worb { plb } else { pla },
				initiator: if worb { Color::White } else { Color::Black },
				state: ChannelGameState::Requested,
				..ChannelGame::new()
			};
			msg.reply(ctx, format!("I've set up your game. You're playing as {}", if worb { "White" } else { "Black" }))?;
		} else {
			msg.reply(ctx, "Who are you playing against? (`c>play @someone`)")?;
		}
	}

	Ok(())
}

#[command]
fn accept(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {

		if gm.state == ChannelGameState::Requested && gm.get_other() == msg.author.id {
			gm.state = ChannelGameState::Running;
			post_board(ctx, &mut gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
		}
	} else {
		msg.reply(ctx, "You haven't been asked to play")?;
	}

	Ok(())
}

#[command]
fn decline(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {
		if gm.state == ChannelGameState::Requested && gm.get_other() == msg.author.id {
			gm.state = ChannelGameState::Inactive;
			msg.reply(ctx, "The table is now open")?;
		}
	} else {
		msg.reply(ctx, "You haven't been asked to play")?;
	}

	Ok(())
}

#[command]
fn cancel(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {
		if gm.state == ChannelGameState::Requested && gm.get_initiator() == msg.author.id {
			msg.channel_id.say(ctx, "The table is now open")?;
			gm.state = ChannelGameState::Inactive;
		}
	} else {
		msg.reply(ctx, "You haven't started a game in this channel")?;
	}

	Ok(())
}

#[command]
#[aliases("stats", "stat")]
fn statistics(ctx: &mut Context, msg: &Message) -> CommandResult {
	USERS.alter(msg.author.id, |opt| match opt {
		Some(stats) => Some(stats),
		None=> Some(UserStats::default())
	});
	let stats = USERS.get(&msg.author.id).unwrap();

	msg.channel(&ctx).unwrap().guild().unwrap().read().send_message(&ctx, |m| m.embed(|embed| {
		embed.colour(serenity::utils::Colour::from_rgb(255, 255, 0));
		embed.field("Games", format!("Total: {}", stats.won_checkmate + stats.won_default + stats.drawn_stalemate + stats.drawn_agreement + stats.drawn_declared + stats.lost_checkmate + stats.lost_resigned), false);
		embed.field(
			"Games won",
			format!("In total: {}\nBy checkmate: {}\nBy default: {}", stats.won_checkmate + stats.won_default, stats.won_checkmate, stats.won_default),
			true
		);
		embed.field(
			"Games drawn",
			format!("In total: {}\nBy stalemate: {}\nBy agreement: {}\nBy declaration: {}", stats.drawn_agreement + stats.drawn_declared + stats.drawn_stalemate, stats.drawn_stalemate, stats.drawn_agreement, stats.drawn_declared),
			true
		);
		embed.field(
			"Games lost",
			format!("In total: {}\nBy checkmate: {}\nBy resignation: {}", stats.lost_checkmate + stats.lost_resigned, stats.lost_checkmate, stats.lost_resigned),
			true
		);
		embed.field("Actions", "_ _", false);
		embed.field(
			"Moves made",
			stats.moves_made,
			true
		);
		embed.field(
			"Pieces captured",
			stats.pieces_captured,
			true
		);
		embed.field(
			"Checks given",
			stats.checks_given,
			true
		);
		embed
	}))?;

	Ok(())
}

#[command]
fn board(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(gm) = GAMES.get_mut(&msg.channel_id) {
		if gm.state != ChannelGameState::Running {
			post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
		}
	} else {
		msg.reply(ctx, "There is no game running")?;
	}

	Ok(())
}

#[command]
fn resign(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {
		if gm.state == ChannelGameState::Running {
			if msg.author.id == gm.white {
				gm.game.resign(Color::White);
				post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				check_game_result(&mut gm);
			} else if msg.author.id == gm.black {
				gm.game.resign(Color::Black);
				post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				check_game_result(&mut gm);
			} else {
				msg.reply(ctx, "You're not playing this game")?;
			}
		}
	} else {
		msg.reply(ctx, "There is no game running")?;
	}

	Ok(())
}

#[command]
fn draw(ctx: &mut Context, msg: &Message) -> CommandResult {
	if let Some(mut gm) = GAMES.get_mut(&msg.channel_id) {
		if gm.state == ChannelGameState::Running {
			if msg.author.id == gm.white {
				if gm.game.side_to_move() == Color::White && gm.game.can_declare_draw() {
					gm.game.declare_draw();
					post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
					check_game_result(&mut gm);
				} else if gm.draw_offer == Some(Color::Black) {
					gm.game.accept_draw();
					post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
					check_game_result(&mut gm);
				} else {
					gm.draw_offer = Some(Color::White);
					gm.game.offer_draw(Color::White);
					msg.reply(ctx, "You have offered a draw")?;
				}
			} else if msg.author.id == gm.black {
				if gm.game.side_to_move() == Color::Black && gm.game.can_declare_draw() {
					gm.game.declare_draw();
					post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
				} else if gm.draw_offer == Some(Color::White) {
					gm.game.accept_draw();
					post_board(ctx, &gm, &msg.channel(&ctx).unwrap().guild().unwrap().read())?;
					check_game_result(&mut gm);
				} else {
					gm.draw_offer = Some(Color::Black);
					gm.game.offer_draw(Color::White);
					msg.reply(ctx, "You have offered a draw")?;
				}
			} else {
				msg.reply(ctx, "You're not playing this game")?;
			}
		}
	} else {
		msg.reply(ctx, "There is no game running")?;
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
	msg.reply(ctx, format!("Succesfully disabled all games in channel <#{}>", msg.channel_id))?;

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
		let old_value = settings.settings.insert(setting.clone(), val).unwrap_or_else(String::new);
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
	setting.retain(|c| !matches!(c, '<' | '>' | '!' | ' '));

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
		let old_value = prefs.settings.insert(setting.clone(), val).unwrap_or_else(String::new);
		msg.reply(ctx, format!("Value of {} set to \"{}\" (previously \"{}\")", setting, prefs.settings.get(&setting).unwrap(), old_value))?;
	} else {
		msg.reply(ctx, format!("Value of {} is: \"{}\"", setting, prefs.settings.get(&setting).unwrap_or(&"".to_string())))?;
	}

	Ok(())
}