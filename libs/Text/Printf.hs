module Text.Printf (printf, hPrintf, PrintfType, PrintfArg(..), UPrintf(..)) where

import Base

data UPrintf = UInteger Integer | UChar Char | UString String | UDouble Double

class PrintfArg a where
  toUPrintf :: a -> UPrintf

instance PrintfArg Int where
  toUPrintf x = UInteger (toInteger x)

instance PrintfArg Integer where
  toUPrintf x = UInteger x

instance PrintfArg Double where
  toUPrintf x = UDouble x

instance PrintfArg Char where
  toUPrintf c = UChar c

instance PrintfArg a => PrintfArg [a] where
  toUPrintf xs = UString (map (\x -> case toUPrintf x of { UChar c -> c; _ -> '?' }) xs)

class PrintfType r where
  spr :: String -> [UPrintf] -> r

instance PrintfType (IO a) where
  spr fmt args = IO (\w -> case primPutStr (uprintf fmt (reverse args)) w of IORes _ -> IORes (primError "printf: unit result"))

instance IsChar c => PrintfType [c] where
  spr fmt args = map fromChar (uprintf fmt (reverse args))

instance (PrintfArg a, PrintfType r) => PrintfType (a -> r) where
  spr fmt args = \a -> spr fmt (toUPrintf a : args)

class IsChar c where
  fromChar :: Char -> c
  toChar :: c -> Char

instance IsChar Char where
  fromChar c = c
  toChar c = c

printf :: PrintfType r => String -> r
printf fmt = spr fmt []

hPrintf :: PrintfType r => a -> String -> r
hPrintf _ fmt = spr fmt []

foreign import prim "doubleFixed" primDoubleFixed :: Int -> Double -> String
foreign import prim "doubleExpFmt" primDoubleExpFmt :: Int -> Double -> String

-- Formats according to a printf format string.
uprintf :: String -> [UPrintf] -> String
uprintf "" _ = ""
uprintf ('%' : '%' : cs) us = '%' : uprintf cs us
uprintf ('%' : cs) us =
  let (flags, cs1) = span (\c -> c == '-' || c == '0' || c == '+' || c == ' ') cs
      (wstr, cs2) = span isDigitC cs1
      (prec, cs3) = case cs2 of
        ('.' : r) -> let (p, r') = span isDigitC r in (Just (if null p then 0 else read p), r')
        _ -> (Nothing, cs2)
  in case cs3 of
       (c : rest) -> case us of
         (u : us') -> pad flags (if null wstr then 0 else read wstr) (fmtOne c prec flags u) ++ uprintf rest us'
         [] -> error "printf: argument list ended prematurely"
       [] -> error "printf: formatting string ended prematurely"
uprintf (c : cs) us = c : uprintf cs us

pad :: String -> Int -> String -> String
pad flags w s =
  let n = length s
  in if n >= w then s
     else if '-' `elem` flags then s ++ replicate (w - n) ' '
     else if '0' `elem` flags then (case s of
                                      ('-' : r) -> '-' : replicate (w - n) '0' ++ r
                                      _ -> replicate (w - n) '0' ++ s)
     else replicate (w - n) ' ' ++ s

fmtOne :: Char -> Maybe Int -> String -> UPrintf -> String
fmtOne c prec flags u = case (c, u) of
  ('d', UInteger n) -> signed flags (show n)
  ('i', UInteger n) -> signed flags (show n)
  ('u', UInteger n) -> show n
  ('x', UInteger n) -> showHex' n
  ('X', UInteger n) -> map toUpper (showHex' n)
  ('o', UInteger n) -> showOct' n
  ('c', UChar ch) -> [ch]
  ('s', UString s) -> maybe s (\p -> take p s) prec
  ('s', UInteger n) -> show n
  ('s', UDouble d) -> show d
  ('v', UInteger n) -> show n
  ('v', UString s) -> s
  ('v', UDouble d) -> show d
  ('v', UChar ch) -> [ch]
  ('f', UDouble d) -> signed flags (primDoubleFixed (maybe 6 id prec) d)
  ('F', UDouble d) -> signed flags (primDoubleFixed (maybe 6 id prec) d)
  ('e', UDouble d) -> signed flags (primDoubleExpFmt (maybe 6 id prec) d)
  ('g', UDouble d) -> signed flags (show d)
  ('f', UInteger n) -> signed flags (primDoubleFixed (maybe 6 id prec) (fromInteger n))
  ('d', UDouble d) -> error "printf: bad formatting char 'd'"
  _ -> error ("printf: bad formatting char " ++ show c)
  where
    signed fl s = if take 1 s == "-" then s
                  else if '+' `elem` fl then '+' : s
                  else if ' ' `elem` fl then ' ' : s
                  else s

showHex' :: Integer -> String
showHex' n = showBase 16 n

showOct' :: Integer -> String
showOct' n = showBase 8 n

showBase :: Integer -> Integer -> String
showBase b n = if n < b then [digit n] else showBase b (n `div` b) ++ [digit (n `mod` b)]
  where digit d = intToDigitP (fromInteger d)

intToDigitP :: Int -> Char
intToDigitP i = if i < 10 then chr (ord '0' + i) else chr (ord 'a' + i - 10)

toUpper :: Char -> Char
toUpper c = if c >= 'a' && c <= 'z' then chr (ord c - 32) else c
