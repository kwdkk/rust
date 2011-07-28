// Type decoding

import std::ivec;
import std::str;
import std::uint;
import std::option;
import std::option::none;
import std::option::some;
import syntax::ast;
import syntax::ast::*;
import ast::respan;
import middle::ty;

export parse_def_id;
export parse_ty_data;

// Compact string representation for ty::t values. API ty_str &
// parse_from_str. Extra parameters are for converting to/from def_ids in the
// data buffer. Whatever format you choose should not contain pipe characters.

// Callback to translate defs to strs or back:
type str_def = fn(str) -> ast::def_id ;

type pstate =
    {data: @u8[], crate: int, mutable pos: uint, len: uint, tcx: ty::ctxt};

tag ty_or_bang { a_ty(ty::t); a_bang; }

fn peek(st: @pstate) -> u8 { ret st.data.(st.pos); }

fn next(st: @pstate) -> u8 {
    let ch = st.data.(st.pos);
    st.pos = st.pos + 1u;
    ret ch;
}

fn parse_ident(st: @pstate, sd: str_def, last: char) -> ast::ident {
    fn is_last(b: char, c: char) -> bool { ret c == b; }
    ret parse_ident_(st, sd, bind is_last(last, _));
}

fn parse_ident_(st: @pstate, sd: str_def, is_last: fn(char) -> bool ) ->
   ast::ident {
    let rslt = "";
    while !is_last(peek(st) as char) {
        rslt += str::unsafe_from_byte(next(st));
    }
    ret rslt;
}


fn parse_ty_data(data: @u8[], crate_num: int, pos: uint, len: uint,
                 sd: str_def, tcx: ty::ctxt) -> ty::t {
    let st =
        @{data: data, crate: crate_num, mutable pos: pos, len: len, tcx: tcx};
    let result = parse_ty(st, sd);
    ret result;
}

fn parse_ty_or_bang(st: @pstate, sd: str_def) -> ty_or_bang {
    alt peek(st) as char {
      '!' { next(st); ret a_bang; }
      _ { ret a_ty(parse_ty(st, sd)); }
    }
}

fn parse_constrs(st: @pstate, sd: str_def) -> (@ty::constr)[] {
    let rslt: (@ty::constr)[] = ~[];
    alt peek(st) as char {
      ':' {
        do  {
            next(st);
            let one: @ty::constr =
                parse_constr[uint](st, sd, parse_constr_arg);
            rslt += ~[one];
        } while peek(st) as char == ';'
      }
      _ { }
    }
    ret rslt;
}

// FIXME less copy-and-paste
fn parse_ty_constrs(st: @pstate, sd: str_def) -> (@ty::type_constr)[] {
    let rslt: (@ty::type_constr)[] = ~[];
    alt peek(st) as char {
      ':' {
        do  {
            next(st);
            let one: @ty::type_constr =
                parse_constr[path](st, sd, parse_ty_constr_arg);
            rslt += ~[one];
        } while peek(st) as char == ';'
      }
      _ { }
    }
    ret rslt;
}

fn parse_path(st: @pstate, sd: str_def) -> ast::path {
    let idents: ast::ident[] = ~[];
    fn is_last(c: char) -> bool { ret c == '(' || c == ':'; }
    idents += ~[parse_ident_(st, sd, is_last)];
    while true {
        alt peek(st) as char {
          ':' { next(st); next(st); }
          c {
            if c == '(' {
                ret respan({lo: 0u, hi: 0u},
                           {global: false, idents: idents, types: ~[]});
            } else { idents += ~[parse_ident_(st, sd, is_last)]; }
          }
        }
    }
    fail "parse_path: ill-formed path";
}

type arg_parser[T] = fn(@pstate, str_def) -> ast::constr_arg_general_[T] ;

fn parse_constr_arg(st: @pstate, sd: str_def) -> ast::fn_constr_arg {
    alt peek(st) as char {
      '*' { st.pos += 1u; ret ast::carg_base; }
      c {

        /* how will we disambiguate between
           an arg index and a lit argument? */
        if c >= '0' && c <= '9' {
            next(st);
            // FIXME
            ret ast::carg_ident((c as uint) - 48u);
        } else {
            log_err "Lit args are unimplemented";
            fail; // FIXME
        }
        /*
          else {
          auto lit = parse_lit(st, sd, ',');
          args += [respan(st.span, ast::carg_lit(lit))];
          }
        */
      }
    }
}

fn parse_ty_constr_arg(st: @pstate, sd: str_def) ->
   ast::constr_arg_general_[path] {
    alt peek(st) as char {
      '*' { st.pos += 1u; ret ast::carg_base; }
      c { ret ast::carg_ident(parse_path(st, sd)); }
    }
}

fn parse_constr[T](st: @pstate, sd: str_def, pser: arg_parser[T]) ->
   @ty::constr_general[T] {
    let sp = {lo: 0u, hi: 0u}; // FIXME: use a real span
    let args: (@sp_constr_arg[T])[] = ~[];
    let pth: path = parse_path(st, sd);
    let ignore: char = next(st) as char;
    assert (ignore as char == '(');
    let def = parse_def(st, sd);
    let an_arg: constr_arg_general_[T];
    do  {
        an_arg = pser(st, sd);
        // FIXME use a real span
        args += ~[@respan(sp, an_arg)];
        ignore = next(st) as char;
    } while ignore == ';'
    assert (ignore == ')');
    ret @respan(sp, {path: pth, args: args, id: def});
}

fn parse_ty(st: @pstate, sd: str_def) -> ty::t {
    alt next(st) as char {
      'n' { ret ty::mk_nil(st.tcx); }
      'z' { ret ty::mk_bot(st.tcx); }
      'b' { ret ty::mk_bool(st.tcx); }
      'i' { ret ty::mk_int(st.tcx); }
      'u' { ret ty::mk_uint(st.tcx); }
      'l' { ret ty::mk_float(st.tcx); }
      'M' {
        alt next(st) as char {
          'b' { ret ty::mk_mach(st.tcx, ast::ty_u8); }
          'w' { ret ty::mk_mach(st.tcx, ast::ty_u16); }
          'l' { ret ty::mk_mach(st.tcx, ast::ty_u32); }
          'd' { ret ty::mk_mach(st.tcx, ast::ty_u64); }
          'B' { ret ty::mk_mach(st.tcx, ast::ty_i8); }
          'W' { ret ty::mk_mach(st.tcx, ast::ty_i16); }
          'L' { ret ty::mk_mach(st.tcx, ast::ty_i32); }
          'D' { ret ty::mk_mach(st.tcx, ast::ty_i64); }
          'f' { ret ty::mk_mach(st.tcx, ast::ty_f32); }
          'F' { ret ty::mk_mach(st.tcx, ast::ty_f64); }
        }
      }
      'c' { ret ty::mk_char(st.tcx); }
      's' { ret ty::mk_str(st.tcx); }
      'S' { ret ty::mk_istr(st.tcx); }
      't' {
        assert (next(st) as char == '[');
        let def = parse_def(st, sd);
        let params: ty::t[] = ~[];
        while peek(st) as char != ']' { params += ~[parse_ty(st, sd)]; }
        st.pos = st.pos + 1u;
        ret ty::mk_tag(st.tcx, def, params);
      }
      'p' {
        let k = alt next(st) as char {
          'u' { kind_unique }
          's' { kind_shared }
          'p' { kind_pinned }
          c {
            log_err "unexpected char in encoded type param: ";
            log_err c; fail
          }
        };
        ret ty::mk_param(st.tcx, parse_int(st) as uint, k);
      }
      '@' { ret ty::mk_box(st.tcx, parse_mt(st, sd)); }
      '*' { ret ty::mk_ptr(st.tcx, parse_mt(st, sd)); }
      'V' { ret ty::mk_vec(st.tcx, parse_mt(st, sd)); }
      'I' { ret ty::mk_ivec(st.tcx, parse_mt(st, sd)); }
      'a' { ret ty::mk_task(st.tcx); }
      'P' { ret ty::mk_port(st.tcx, parse_ty(st, sd)); }
      'C' { ret ty::mk_chan(st.tcx, parse_ty(st, sd)); }
      'R' {
        assert (next(st) as char == '[');
        let fields: ty::field[] = ~[];
        while peek(st) as char != ']' {
            let name = "";
            while peek(st) as char != '=' {
                name += str::unsafe_from_byte(next(st));
            }
            st.pos = st.pos + 1u;
            fields += ~[{ident: name, mt: parse_mt(st, sd)}];
        }
        st.pos = st.pos + 1u;
        ret ty::mk_rec(st.tcx, fields);
      }
      'F' {
        let func = parse_ty_fn(st, sd);
        ret ty::mk_fn(st.tcx, ast::proto_fn, func.args, func.ty, func.cf,
                      func.cs);
      }
      'W' {
        let func = parse_ty_fn(st, sd);
        ret ty::mk_fn(st.tcx, ast::proto_iter, func.args, func.ty, func.cf,
                      func.cs);
      }
      'B' {
        let func = parse_ty_fn(st, sd);
        ret ty::mk_fn(st.tcx, ast::proto_block, func.args, func.ty, func.cf,
                      func.cs);
      }
      'N' {
        let abi;
        alt next(st) as char {
          'r' { abi = ast::native_abi_rust; }
          'i' { abi = ast::native_abi_rust_intrinsic; }
          'c' { abi = ast::native_abi_cdecl; }
          'l' { abi = ast::native_abi_llvm; }
          's' { abi = ast::native_abi_x86stdcall; }
        }
        let func = parse_ty_fn(st, sd);
        ret ty::mk_native_fn(st.tcx, abi, func.args, func.ty);
      }
      'O' {
        assert (next(st) as char == '[');
        let methods: ty::method[] = ~[];
        while peek(st) as char != ']' {
            let proto;
            alt next(st) as char {
              'W' { proto = ast::proto_iter; }
              'F' { proto = ast::proto_fn; }
            }
            let name = "";
            while peek(st) as char != '[' {
                name += str::unsafe_from_byte(next(st));
            }
            let func = parse_ty_fn(st, sd);
            methods +=
                ~[{proto: proto,
                   ident: name,
                   inputs: func.args,
                   output: func.ty,
                   cf: func.cf,
                   constrs: func.cs}];
        }
        st.pos += 1u;
        ret ty::mk_obj(st.tcx, methods);
      }
      'r' {
        assert (next(st) as char == '[');
        let def = parse_def(st, sd);
        let inner = parse_ty(st, sd);
        let params: ty::t[] = ~[];
        while peek(st) as char != ']' { params += ~[parse_ty(st, sd)]; }
        st.pos = st.pos + 1u;
        ret ty::mk_res(st.tcx, def, inner, params);
      }
      'X' { ret ty::mk_var(st.tcx, parse_int(st)); }
      'E' { let def = parse_def(st, sd); ret ty::mk_native(st.tcx, def); }
      'Y' { ret ty::mk_type(st.tcx); }
      '#' {
        let pos = parse_hex(st);
        assert (next(st) as char == ':');
        let len = parse_hex(st);
        assert (next(st) as char == '#');
        alt st.tcx.rcache.find({cnum: st.crate, pos: pos, len: len}) {
          some(tt) { ret tt; }
          none. {
            let ps = @{pos: pos, len: len with *st};
            let tt = parse_ty(ps, sd);
            st.tcx.rcache.insert({cnum: st.crate, pos: pos, len: len}, tt);
            ret tt;
          }
        }
      }
      'A' {
        assert (next(st) as char == '[');
        let tt = parse_ty(st, sd);
        let tcs = parse_ty_constrs(st, sd);
        assert (next(st) as char == ']');
        ret ty::mk_constr(st.tcx, tt, tcs);
      }
      c { log_err "unexpected char in type string: "; log_err c; fail; }
    }
}

fn parse_mt(st: @pstate, sd: str_def) -> ty::mt {
    let mut;
    alt peek(st) as char {
      'm' { next(st); mut = ast::mut; }
      '?' { next(st); mut = ast::maybe_mut; }
      _ { mut = ast::imm; }
    }
    ret {ty: parse_ty(st, sd), mut: mut};
}

fn parse_def(st: @pstate, sd: str_def) -> ast::def_id {
    let def = "";
    while peek(st) as char != '|' { def += str::unsafe_from_byte(next(st)); }
    st.pos = st.pos + 1u;
    ret sd(def);
}

fn parse_int(st: @pstate) -> int {
    let n = 0;
    while true {
        let cur = peek(st) as char;
        if cur < '0' || cur > '9' { break; }
        st.pos = st.pos + 1u;
        n *= 10;
        n += (cur as int) - ('0' as int);
    }
    ret n;
}

fn parse_hex(st: @pstate) -> uint {
    let n = 0u;
    while true {
        let cur = peek(st) as char;
        if (cur < '0' || cur > '9') && (cur < 'a' || cur > 'f') { break; }
        st.pos = st.pos + 1u;
        n *= 16u;
        if '0' <= cur && cur <= '9' {
            n += (cur as uint) - ('0' as uint);
        } else { n += 10u + (cur as uint) - ('a' as uint); }
    }
    ret n;
}

fn parse_ty_fn(st: @pstate, sd: str_def) ->
   {args: ty::arg[], ty: ty::t, cf: ast::controlflow, cs: (@ty::constr)[]} {
    assert (next(st) as char == '[');
    let inputs: ty::arg[] = ~[];
    while peek(st) as char != ']' {
        let mode = ty::mo_val;
        if peek(st) as char == '&' {
            mode = ty::mo_alias(false);
            st.pos += 1u;
            if peek(st) as char == 'm' {
                mode = ty::mo_alias(true);
                st.pos += 1u;
            }
        }
        inputs += ~[{mode: mode, ty: parse_ty(st, sd)}];
    }
    st.pos += 1u; // eat the ']'
    let cs = parse_constrs(st, sd);
    alt parse_ty_or_bang(st, sd) {
      a_bang. {
        ret {args: inputs, ty: ty::mk_bot(st.tcx), cf: ast::noreturn, cs: cs};
      }
      a_ty(t) { ret {args: inputs, ty: t, cf: ast::return, cs: cs}; }
    }
}


// Rust metadata parsing
fn parse_def_id(buf: &u8[]) -> ast::def_id {
    let colon_idx = 0u;
    let len = ivec::len[u8](buf);
    while colon_idx < len && buf.(colon_idx) != ':' as u8 { colon_idx += 1u; }
    if colon_idx == len {
        log_err "didn't find ':' when parsing def id";
        fail;
    }
    let crate_part = ivec::slice[u8](buf, 0u, colon_idx);
    let def_part = ivec::slice[u8](buf, colon_idx + 1u, len);

    // FIXME: Remove these ivec->vec conversions.
    let crate_part_vec = [];
    let def_part_vec = [];
    for b: u8  in crate_part { crate_part_vec += [b]; }
    for b: u8  in def_part { def_part_vec += [b]; }

    let crate_num = uint::parse_buf(crate_part_vec, 10u) as int;
    let def_num = uint::parse_buf(def_part_vec, 10u) as int;
    ret {crate: crate_num, node: def_num};
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
