/// Precomputed piece attack bitboards.
///
/// Each u64 bitboard has bit N set if a piece on square N can attack that square.
/// Square numbering: a8=0, b8=1, ..., h8=7, a7=8, ..., h1=63.
/// (rank = sq/8 where 0=rank8, 7=rank1; file = sq%8 where 0=a, 7=h)
///
/// Pawn tables are stored for white only. Use `flip_rank_sq` / `flip_rank_bb` for black.
/// Sliding tables store full rays (ignoring occupancy). At runtime, mask with occupancy
/// to find actual reachable squares.

// Const helpers

pub const fn sq_rank(sq: u8) -> i8 {
    sq as i8 / 8
}

pub const fn sq_file(sq: u8) -> i8 {
    sq as i8 % 8
}

pub const fn rank_file_to_sq(rank: i8, file: i8) -> u8 {
    (rank * 8 + file) as u8
}

const fn in_bounds(rank: i8, file: i8) -> bool {
    rank >= 0 && rank < 8 && file >= 0 && file < 8
}

const fn try_set(table: &mut [u64; 64], sq: u8, rank: i8, file: i8) {
    if in_bounds(rank, file) {
        table[sq as usize] |= 1u64 << rank_file_to_sq(rank, file);
    }
}

// Flip helpers (mirror vertically for black)

pub const fn flip_rank_sq(sq: u8) -> u8 {
    rank_file_to_sq(7 - sq_rank(sq), sq_file(sq))
}

pub const fn flip_rank_bb(bb: u64) -> u64 {
    let mut result = 0u64;
    let mut sq = 0u8;
    while sq < 64 {
        if bb & (1u64 << sq) != 0 {
            result |= 1u64 << flip_rank_sq(sq);
        }
        sq += 1;
    }
    result
}

// Table builders

const fn build_knight_attacks() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        try_set(&mut table, sq, r - 2, f - 1);
        try_set(&mut table, sq, r - 2, f + 1);
        try_set(&mut table, sq, r - 1, f - 2);
        try_set(&mut table, sq, r - 1, f + 2);
        try_set(&mut table, sq, r + 1, f - 2);
        try_set(&mut table, sq, r + 1, f + 2);
        try_set(&mut table, sq, r + 2, f - 1);
        try_set(&mut table, sq, r + 2, f + 1);
        sq += 1;
    }
    table
}

const fn build_king_attacks() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        try_set(&mut table, sq, r - 1, f - 1);
        try_set(&mut table, sq, r - 1, f);
        try_set(&mut table, sq, r - 1, f + 1);
        try_set(&mut table, sq, r, f - 1);
        try_set(&mut table, sq, r, f + 1);
        try_set(&mut table, sq, r + 1, f - 1);
        try_set(&mut table, sq, r + 1, f);
        try_set(&mut table, sq, r + 1, f + 1);
        sq += 1;
    }
    table
}

const fn build_pawn_attacks_white() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        if r > 0 {
            try_set(&mut table, sq, r - 1, f - 1);
            try_set(&mut table, sq, r - 1, f + 1);
        }
        sq += 1;
    }
    table
}

const fn build_pawn_pushes_white() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        if r > 0 {
            table[sq as usize] |= 1u64 << rank_file_to_sq(r - 1, f);
            if r == 6 {
                table[sq as usize] |= 1u64 << rank_file_to_sq(r - 2, f);
            }
        }
        sq += 1;
    }
    table
}

const fn walk_ray(table: &mut [u64; 64], sq: u8, start_r: i8, start_f: i8, dr: i8, df: i8) {
    let mut r = start_r + dr;
    let mut f = start_f + df;
    while in_bounds(r, f) {
        table[sq as usize] |= 1u64 << rank_file_to_sq(r, f);
        r += dr;
        f += df;
    }
}

const fn build_bishop_rays() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        walk_ray(&mut table, sq, r, f, -1, -1);
        walk_ray(&mut table, sq, r, f, -1, 1);
        walk_ray(&mut table, sq, r, f, 1, -1);
        walk_ray(&mut table, sq, r, f, 1, 1);
        sq += 1;
    }
    table
}

const fn build_rook_rays() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let r = sq_rank(sq);
        let f = sq_file(sq);
        walk_ray(&mut table, sq, r, f, -1, 0);
        walk_ray(&mut table, sq, r, f, 1, 0);
        walk_ray(&mut table, sq, r, f, 0, -1);
        walk_ray(&mut table, sq, r, f, 0, 1);
        sq += 1;
    }
    table
}

const fn build_queen_rays() -> [u64; 64] {
    let bishop = build_bishop_rays();
    let rook = build_rook_rays();
    let mut table = [0u64; 64];
    let mut i = 0;
    while i < 64 {
        table[i] = bishop[i] | rook[i];
        i += 1;
    }
    table
}

// Precomputed tables (computed at compile time)

pub const KNIGHT_ATTACKS: [u64; 64] = build_knight_attacks();
pub const KING_ATTACKS: [u64; 64] = build_king_attacks();
pub const PAWN_ATTACKS_WHITE: [u64; 64] = build_pawn_attacks_white();
pub const PAWN_PUSHES_WHITE: [u64; 64] = build_pawn_pushes_white();
pub const BISHOP_RAYS: [u64; 64] = build_bishop_rays();
pub const ROOK_RAYS: [u64; 64] = build_rook_rays();
pub const QUEEN_RAYS: [u64; 64] = build_queen_rays();

// Bitboard iteration

pub struct BitboardIter(u64);

impl Iterator for BitboardIter {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.0 == 0 {
            return None;
        }
        let sq = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(sq)
    }
}

pub fn bitboard_iter(bb: u64) -> BitboardIter {
    BitboardIter(bb)
}

// Sliding piece attack generation at runtime

pub fn sliding_attacks(sq: u8, ray_table: &[u64; 64], occupancy: u64) -> u64 {
    let _rays = ray_table[sq as usize];
    let mut attacks = 0u64;
    let r = sq_rank(sq);
    let f = sq_file(sq);

    let directions: [(i8, i8); 8] = [
        (-1, 0), (1, 0), (0, -1), (0, 1),
        (-1, -1), (-1, 1), (1, -1), (1, 1),
    ];

    let mut d = 0;
    while d < 8 {
        let dr = directions[d].0;
        let df = directions[d].1;
        let mut nr = r + dr;
        let mut nf = f + df;
        while in_bounds(nr, nf) {
            let s = rank_file_to_sq(nr, nf);
            let bit = 1u64 << s;
            attacks |= bit;
            if occupancy & bit != 0 {
                break;
            }
            nr += dr;
            nf += df;
        }
        d += 1;
    }

    attacks
}
