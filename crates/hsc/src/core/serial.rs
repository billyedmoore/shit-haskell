//! Binary serialisation of Core programs, used to embed a compiled program in its
//! output executable.

use super::{Alt, Core, Lit, TopBind};

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_lit(out: &mut Vec<u8>, l: &Lit) {
    match l {
        Lit::Integer(s) => {
            out.push(0);
            put_str(out, s);
        }
        Lit::Frac(s) => {
            out.push(1);
            put_str(out, s);
        }
        Lit::Char(c) => {
            out.push(2);
            out.extend_from_slice(&(*c as u32).to_le_bytes());
        }
        Lit::Str(s) => {
            out.push(3);
            put_str(out, s);
        }
        Lit::Int(i) => {
            out.push(4);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Lit::Addr(s) => {
            out.push(6);
            put_str(out, s);
        }
        Lit::Double(d) => {
            out.push(5);
            out.extend_from_slice(&d.to_bits().to_le_bytes());
        }
    }
}

fn put_core(out: &mut Vec<u8>, c: &Core) {
    match c {
        Core::Var(v) => {
            out.push(0);
            put_str(out, v);
        }
        Core::Lit(l) => {
            out.push(1);
            put_lit(out, l);
        }
        Core::App(f, a) => {
            out.push(2);
            put_core(out, f);
            put_core(out, a);
        }
        Core::Lam(v, b) => {
            out.push(3);
            put_str(out, v);
            put_core(out, b);
        }
        Core::Let(binds, body) => {
            out.push(4);
            put_u32(out, binds.len());
            for (n, b) in binds {
                put_str(out, n);
                put_core(out, b);
            }
            put_core(out, body);
        }
        Core::Case(s, b, alts) => {
            out.push(5);
            put_core(out, s);
            put_str(out, b);
            put_u32(out, alts.len());
            for a in alts {
                match a {
                    Alt::Con(name, vars, body) => {
                        out.push(0);
                        put_str(out, name);
                        put_u32(out, vars.len());
                        for v in vars {
                            put_str(out, v);
                        }
                        put_core(out, body);
                    }
                    Alt::Lit(l, body) => {
                        out.push(1);
                        put_lit(out, l);
                        put_core(out, body);
                    }
                    Alt::Default(body) => {
                        out.push(2);
                        put_core(out, body);
                    }
                }
            }
        }
        Core::Con(name, args) => {
            out.push(6);
            put_str(out, name);
            put_u32(out, args.len());
            for a in args {
                put_core(out, a);
            }
        }
        Core::Prim(name, args) => {
            out.push(7);
            put_str(out, name);
            put_u32(out, args.len());
            for a in args {
                put_core(out, a);
            }
        }
        Core::Fail(msg) => {
            out.push(8);
            put_str(out, msg);
        }
    }
}

fn put_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

pub fn encode(binds: &[TopBind]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, binds.len());
    for b in binds {
        put_str(&mut out, &b.name);
        put_core(&mut out, &b.body);
    }
    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.pos).ok_or("truncated program")?;
        self.pos += 1;
        Ok(b)
    }
    fn u32(&mut self) -> Result<usize, String> {
        let bytes = self.data.get(self.pos..self.pos + 4).ok_or("truncated program")?;
        self.pos += 4;
        Ok(u32::from_le_bytes(bytes.try_into().expect("4 bytes")) as usize)
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.u32()?;
        let bytes = self.data.get(self.pos..self.pos + n).ok_or("truncated program")?;
        self.pos += n;
        String::from_utf8(bytes.to_vec()).map_err(|_| "invalid UTF-8 in program".to_string())
    }
    fn lit(&mut self) -> Result<Lit, String> {
        Ok(match self.byte()? {
            0 => Lit::Integer(self.string()?),
            1 => Lit::Frac(self.string()?),
            2 => Lit::Char(char::from_u32(self.u32()? as u32).ok_or("bad char")?),
            3 => Lit::Str(self.string()?),
            6 => Lit::Addr(self.string()?),
            4 => {
                let b = self.data.get(self.pos..self.pos + 8).ok_or("truncated program")?;
                self.pos += 8;
                Lit::Int(i64::from_le_bytes(b.try_into().expect("8 bytes")))
            }
            5 => {
                let b = self.data.get(self.pos..self.pos + 8).ok_or("truncated program")?;
                self.pos += 8;
                Lit::Double(f64::from_bits(u64::from_le_bytes(b.try_into().expect("8 bytes"))))
            }
            _ => return Err("bad literal tag".into()),
        })
    }
    fn core(&mut self) -> Result<Core, String> {
        Ok(match self.byte()? {
            0 => Core::Var(self.string()?),
            1 => Core::Lit(self.lit()?),
            2 => Core::App(Box::new(self.core()?), Box::new(self.core()?)),
            3 => {
                let v = self.string()?;
                Core::Lam(v, Box::new(self.core()?))
            }
            4 => {
                let n = self.u32()?;
                let mut binds = Vec::with_capacity(n);
                for _ in 0..n {
                    let name = self.string()?;
                    binds.push((name, self.core()?));
                }
                Core::Let(binds, Box::new(self.core()?))
            }
            5 => {
                let s = self.core()?;
                let b = self.string()?;
                let n = self.u32()?;
                let mut alts = Vec::with_capacity(n);
                for _ in 0..n {
                    alts.push(match self.byte()? {
                        0 => {
                            let name = self.string()?;
                            let nv = self.u32()?;
                            let mut vars = Vec::with_capacity(nv);
                            for _ in 0..nv {
                                vars.push(self.string()?);
                            }
                            Alt::Con(name, vars, self.core()?)
                        }
                        1 => {
                            let l = self.lit()?;
                            Alt::Lit(l, self.core()?)
                        }
                        2 => Alt::Default(self.core()?),
                        _ => return Err("bad alternative tag".into()),
                    });
                }
                Core::Case(Box::new(s), b, alts)
            }
            6 | 7 => {
                let tag = self.data[self.pos - 1];
                let name = self.string()?;
                let n = self.u32()?;
                let mut args = Vec::with_capacity(n);
                for _ in 0..n {
                    args.push(self.core()?);
                }
                if tag == 6 {
                    Core::Con(name, args)
                } else {
                    Core::Prim(name, args)
                }
            }
            8 => Core::Fail(self.string()?),
            _ => return Err("bad core tag".into()),
        })
    }
}

pub fn decode(data: &[u8]) -> Result<Vec<TopBind>, String> {
    let mut r = Reader { data, pos: 0 };
    let n = r.u32()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.string()?;
        let body = r.core()?;
        out.push(TopBind { name, body });
    }
    Ok(out)
}
