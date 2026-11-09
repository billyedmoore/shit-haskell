module Text.Read (Read(..), ReadS, read, reads, readMaybe, readEither) where

import Base

readMaybe :: Read a => String -> Maybe a
readMaybe s = case [x | (x, rest) <- readsPrec 0 s, all isSpaceC rest] of
  (x : _) -> Just x
  [] -> Nothing

readEither :: Read a => String -> Either String a
readEither s = case readMaybe s of
  Just x -> Right x
  Nothing -> Left "Prelude.read: no parse"
