{-# LANGUAGE MagicHash #-}
-- Inline Char#, Word# and tagToEnum# code, static small Integer literals, and the folds of
-- Int/Word/Integer conversions: results must match the runtime's primitives.
import Data.Char (toUpper, isSpace, ord, chr)
import Data.Word (Word)
import GHC.Exts

cmpChars :: Char -> Char -> [Bool]
cmpChars a b = [a == b, a /= b, a < b, a <= b, a > b, a >= b]

cmpWords :: Word -> Word -> [Bool]
cmpWords a b = [a == b, a /= b, a < b, a <= b, a > b, a >= b]

-- A comparison result returned as a Bool value (not scrutinised in place).
gtBool :: Int -> Int -> Bool
gtBool (I# a) (I# b) = isTrue# (a ># b)
{-# NOINLINE gtBool #-}

big :: Integer
big = 9223372036854775807

main :: IO ()
main = do
  print (cmpChars 'a' 'b', cmpChars '\1114111' 'a', cmpChars 'x' 'x')
  print (map toUpper "hello, World! \233\223", filter (not . isSpace) " a\tb\nc\160d ")
  print (map ord "az\0\1114111", map chr [65, 0x1F600, 0])
  print (cmpWords 1 maxBound, cmpWords maxBound 0, cmpWords 7 7)
  print (maxBound + (1 :: Word), (0 :: Word) - 1, (3 :: Word) * maxBound)
  print (fromIntegral (ord 'z') :: Word, fromIntegral (-1 :: Int) :: Word, fromIntegral (maxBound :: Word) :: Int)
  print (gtBool 3 2, gtBool 2 3, [gtBool x 5 | x <- [4, 5, 6]])
  print (big, big + 1, negate big - 2, 12345678901234567890123 :: Integer)
  print (sum [1 .. 100 :: Integer], product [1 .. 25 :: Integer], 2 ^ (70 :: Int) :: Integer)
  print (fromIntegral (big + 1) :: Int, fromInteger 300 :: Word, toInteger (minBound :: Int))
