module Data.Char (
    Char, ord, chr, isDigit, isAlpha, isLetter, isSpace, isUpper, isLower,
    isAlphaNum, isPunctuation, isControl, isPrint, isHexDigit, isOctDigit, isAscii,
    isSymbol, isSeparator, isNumber, isMark, toUpper, toLower, digitToInt, intToDigit,
    isAsciiUpper, isAsciiLower
  ) where

import Base

isDigit :: Char -> Bool
isDigit c = c >= '0' && c <= '9'

isOctDigit :: Char -> Bool
isOctDigit c = c >= '0' && c <= '7'

isHexDigit :: Char -> Bool
isHexDigit c = isDigit c || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')

isAlpha :: Char -> Bool
isAlpha c = isLower c || isUpper c

isLetter :: Char -> Bool
isLetter = isAlpha

isUpper :: Char -> Bool
isUpper c = c >= 'A' && c <= 'Z'

isLower :: Char -> Bool
isLower c = c >= 'a' && c <= 'z'

isAsciiUpper :: Char -> Bool
isAsciiUpper = isUpper

isAsciiLower :: Char -> Bool
isAsciiLower = isLower

isAlphaNum :: Char -> Bool
isAlphaNum c = isAlpha c || isDigit c

isNumber :: Char -> Bool
isNumber = isDigit

isSpace :: Char -> Bool
isSpace c = c == ' ' || (c >= '\t' && c <= '\r') || c == '\160'

isPunctuation :: Char -> Bool
isPunctuation c = c `elem` "!\"#%&'()*,-./:;?@[\\]_{}"

isSymbol :: Char -> Bool
isSymbol c = c `elem` "$+<=>^`|~"

isSeparator :: Char -> Bool
isSeparator c = c == ' '

isMark :: Char -> Bool
isMark _ = False

isControl :: Char -> Bool
isControl c = ord c < 32 || ord c == 127

isPrint :: Char -> Bool
isPrint c = not (isControl c)

isAscii :: Char -> Bool
isAscii c = ord c < 128

toUpper :: Char -> Char
toUpper c = if isLower c then chr (ord c - 32) else c

toLower :: Char -> Char
toLower c = if isUpper c then chr (ord c + 32) else c

digitToInt :: Char -> Int
digitToInt c
  | isDigit c = ord c - ord '0'
  | c >= 'a' && c <= 'f' = ord c - ord 'a' + 10
  | c >= 'A' && c <= 'F' = ord c - ord 'A' + 10
  | otherwise = error ("Char.digitToInt: not a digit " ++ show c)

intToDigit :: Int -> Char
intToDigit i
  | i >= 0 && i <= 9 = chr (ord '0' + i)
  | i >= 10 && i <= 15 = chr (ord 'a' + i - 10)
  | otherwise = error ("Char.intToDigit: not a digit " ++ show i)
