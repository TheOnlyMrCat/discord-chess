use chess::*;
use serenity::model::id::{UserId, MessageId};

pub struct ChannelGame {
	pub game: Game,
	pub running: bool,
	pub finished: bool,
	pub old_boards: Vec<MessageId>,
	pub white: UserId,
	pub black: UserId,
	pub initiator: Color
}

impl ChannelGame {
	pub fn new() -> ChannelGame {
		ChannelGame {
			game: Game::new(),
			running: false,
			finished: false,
			old_boards: Vec::new(),
			white: UserId::default(),
			black: UserId::default(),
			initiator: Color::White,
		}
	}

	#[inline]
	pub fn get_initiator(&self) -> UserId {
		match self.initiator {
			Color::White => self.white,
			Color::Black => self.black
		}
	}

	#[inline]
	pub fn get_other(&self) -> UserId {
		match self.initiator {
			Color::White => self.black,
			Color::Black => self.white
		}
	}
}

pub trait FromSan {
	fn from_san(board: &Board, move_text: &str) -> Result<ChessMove, Error>;
}

impl FromSan for ChessMove {
	//* Ripped from https://github.com/jordanbray/chess/blob/master/src/chess_move.rs
	//* Not my algorithm

	/// Convert a SAN (Standard Algebraic Notation) move into a `ChessMove`
	///
	/// ```
	/// use chess::{Board, ChessMove, Square};
	///
	/// let board = Board::default();
	/// assert_eq!(
	///     ChessMove::from_san(&board, "e4").expect("e4 is valid in the initial position"),
	///     ChessMove::new(Square::E2, Square::E4, None)
	/// );
	/// ```
	fn from_san(board: &Board, move_text: &str) -> Result<ChessMove, Error> {
		// Castles first...
		if move_text == "O-O" || move_text == "O-O-O" {
			let rank = board.side_to_move().to_my_backrank();
			let source_file = File::E;
			let dest_file = if move_text == "O-O" { File::G } else { File::C };

			let m = ChessMove::new(
				Square::make_square(rank, source_file),
				Square::make_square(rank, dest_file),
				None,
			);
			if MoveGen::new_legal(&board).any(|l| l == m) {
				return Ok(m);
			} else {
				return Err(Error::InvalidBoard);
			}
		}

		// forms of SAN moves
		// a4 (Pawn moves to a4)
		// exd4 (Pawn on e file takes on d4)
		// xd4 (Illegal, source file must be specified)
		// 1xd4 (Illegal, source file (not rank) must be specified)
		// Nc3 (Knight (or any piece) on *some square* to c3
		// Nb1c3 (Knight (or any piece) on b1 to c3
		// Nbc3 (Knight on b file to c3)
		// N1c3 (Knight on first rank to c3)
		// Nb1xc3 (Knight on b1 takes on c3)
		// Nbxc3 (Knight on b file takes on c3)
		// N1xc3 (Knight on first rank takes on c3)
		// Nc3+ (Knight moves to c3 with check)
		// Nc3# (Knight moves to c3 with checkmate)

		// Because I'm dumb, I'm wondering if a hash table of all possible moves would be stupid.
		// There are only 186624 possible moves in SAN notation.
		//
		// Would this even be faster?  Somehow I doubt it because caching, but maybe, I dunno...
		// This could take the form of a:
		// struct CheckOrCheckmate {
		//      Neither,
		//      Check,
		//      CheckMate,
		// }
		// struct FromSan {
		//      piece: Piece,
		//      source: Vec<Square>, // possible source squares
		//      // OR
		//      source_rank: Option<Rank>,
		//      source_file: Option<File>,
		//      dest: Square,
		//      takes: bool,
		//      check: CheckOrCheckmate
		// }
		//
		// This could be kept internally as well, and never tell the user about such an abomination
		//
		// I estimate this table would take around 2 MiB, but I had to approximate some things.  It
		// may be less

		// This can be described with the following format
		// [Optional Piece Specifier] ("" | "N" | "B" | "R" | "Q" | "K")
		// [Optional Source Specifier] ( "" | "a-h" | "1-8" | ("a-h" + "1-8"))
		// [Optional Takes Specifier] ("" | "x")
		// [Full Destination Square] ("a-h" + "0-8")
		// [Optional Promotion Specifier] ("" | "=N" | "=B" | "=R" | "=Q")
		// [Optional Check(mate) Specifier] ("" | "+" | "#")
		// [Optional En Passant Specifier] ("" | " e.p.")

		let mut cur_index: usize = 0;
		let moving_piece = match move_text
			.get(cur_index..(cur_index + 1))
			.ok_or(Error::InvalidBoard)?
		{
			"N" => {
				cur_index += 1;
				Piece::Knight
			}
			"B" => {
				cur_index += 1;
				Piece::Bishop
			}
			"Q" => {
				cur_index += 1;
				Piece::Queen
			}
			"R" => {
				cur_index += 1;
				Piece::Rook
			}
			"K" => {
				cur_index += 1;
				Piece::King
			}
			_ => Piece::Pawn,
		};

		let mut source_file = match move_text
			.get(cur_index..(cur_index + 1))
			.ok_or(Error::InvalidBoard)?
		{
			"a" => {
				cur_index += 1;
				Some(File::A)
			}
			"b" => {
				cur_index += 1;
				Some(File::B)
			}
			"c" => {
				cur_index += 1;
				Some(File::C)
			}
			"d" => {
				cur_index += 1;
				Some(File::D)
			}
			"e" => {
				cur_index += 1;
				Some(File::E)
			}
			"f" => {
				cur_index += 1;
				Some(File::F)
			}
			"g" => {
				cur_index += 1;
				Some(File::G)
			}
			"h" => {
				cur_index += 1;
				Some(File::H)
			}
			_ => None,
		};

		let mut source_rank = match move_text
			.get(cur_index..(cur_index + 1))
			.ok_or(Error::InvalidBoard)?
		{
			"1" => {
				cur_index += 1;
				Some(Rank::First)
			}
			"2" => {
				cur_index += 1;
				Some(Rank::Second)
			}
			"3" => {
				cur_index += 1;
				Some(Rank::Third)
			}
			"4" => {
				cur_index += 1;
				Some(Rank::Fourth)
			}
			"5" => {
				cur_index += 1;
				Some(Rank::Fifth)
			}
			"6" => {
				cur_index += 1;
				Some(Rank::Sixth)
			}
			"7" => {
				cur_index += 1;
				Some(Rank::Seventh)
			}
			"8" => {
				cur_index += 1;
				Some(Rank::Eighth)
			}
			_ => None,
		};

		let takes = if let Some(s) = move_text.get(cur_index..(cur_index + 1)) {
			match s {
				"x" => {
					cur_index += 1;
					true
				}
				_ => false,
			}
		} else {
			false
		};

		let dest = if let Some(s) = move_text.get(cur_index..(cur_index + 2)) {
			if let Some(q) = Square::from_string(String::from(s)) {
				cur_index += 2;
				q
			} else {
				let sq = Square::make_square(
					source_rank.ok_or(Error::InvalidBoard)?,
					source_file.ok_or(Error::InvalidBoard)?,
				);
				source_rank = None;
				source_file = None;
				sq
			}
		} else {
			let sq = Square::make_square(
				source_rank.ok_or(Error::InvalidBoard)?,
				source_file.ok_or(Error::InvalidBoard)?,
			);
			source_rank = None;
			source_file = None;
			sq
		};

		let promotion = if let Some(n) = move_text.get(cur_index..(cur_index + 1)) {
			if n == "=" {
				if let Some(s) = move_text.get((cur_index + 1)..(cur_index + 2)) {
					match s {
						"N" => {
							cur_index += 2;
							Some(Piece::Knight)
						}
						"B" => {
							cur_index += 2;
							Some(Piece::Bishop)
						}
						"R" => {
							cur_index += 2;
							Some(Piece::Rook)
						}
						"Q" => {
							cur_index += 2;
							Some(Piece::Queen)
						}
						_ => None,
					}
				} else {
					None
				}
			} else {
				None
			}
		} else {
			None
		};

		if let Some(s) = move_text.get(cur_index..(cur_index + 1)) {
			let _maybe_check_or_mate = match s {
				"+" => {
					cur_index += 1;
					Some(false)
				}
				"#" => {
					cur_index += 1;
					Some(true)
				}
				_ => None,
			};
		}

		let ep = if let Some(s) = move_text.get(cur_index..) {
			s == " e.p."
		} else {
			false
		};

		//if ep {
		//    cur_index += 5;
		//}

		// Ok, now we have all the data from the SAN move, in the following structures
		// moveing_piece, source_rank, source_file, taks, dest, promotion, maybe_check_or_mate, and
		// ep

		let mut found_move: Option<ChessMove> = None;
		for m in &mut MoveGen::new_legal(board) {
			// check that the move has the properties specified
			if board.piece_on(m.get_source()) != Some(moving_piece) {
				continue;
			}

			if let Some(rank) = source_rank {
				if m.get_source().get_rank() != rank {
					continue;
				}
			}

			if let Some(file) = source_file {
				if m.get_source().get_file() != file {
					continue;
				}
			}

			if m.get_dest() != dest {
				continue;
			}

			if m.get_promotion() != promotion {
				continue;
			}

			if found_move.is_some() {
				return Err(Error::InvalidFen{fen: String::from("")});
			}

			// takes is complicated, because of e.p.
			if !takes {
				if board.piece_on(m.get_dest()).is_some() {
					continue;
				}
			}

			if !ep && takes {
				if board.piece_on(m.get_dest()).is_none() {
					continue;
				}
			}

			found_move = Some(m);
		}

		found_move.ok_or(Error::InvalidFen{fen: String::from("")})
	}
}