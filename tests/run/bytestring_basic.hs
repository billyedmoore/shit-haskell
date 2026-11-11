{-# LANGUAGE OverloadedStrings #-}
import qualified Data.ByteString as B
import qualified Data.ByteString.Char8 as C
import qualified Data.ByteString.Lazy as L
import qualified Data.ByteString.Lazy.Char8 as LC
import qualified Data.ByteString.Builder as BB
import Data.Word (Word8)

main :: IO ()
main = do
  let s = C.pack "hello, bytestring world"
  print s
  print (B.length s, C.count 'o' s, C.elemIndex 'w' s, C.words s)
  print (B.unpack (B.take 5 s), C.map succ (C.take 5 s), C.reverse s)
  print (C.split ',' s, C.isPrefixOf "hello" s, C.isSuffixOf "world" s, C.breakSubstring "byte" s)
  print (B.pack [104, 105 :: Word8], B.cons 65 (B.singleton 66), B.append s "!", B.concat ["a", "b", "c"])
  print (C.readInt "1234 rest", C.readInt "x", C.unwords (map (C.pack . show) [1 .. 5 :: Int]))
  print (B.foldl' (\a w -> a + fromIntegral w) (0 :: Int) s, B.maximum s, B.minimum s, B.sort (C.pack "banana"))
  print (compare (C.pack "abc") (C.pack "abd"), C.pack "x" == C.pack "x", B.null B.empty)
  let l = LC.pack (concat (replicate 3 "lazy "))
  print (l, L.length l, LC.toStrict (LC.take 9 l), L.toChunks (L.fromChunks ["ab", "cd"]))
  let b = BB.intDec 42 <> BB.char7 ' ' <> BB.string7 "built" <> BB.char7 ' ' <> BB.wordHex 255 <> BB.char7 ' ' <> BB.doubleDec 1.5
  print (BB.toLazyByteString b)
  C.putStrLn (C.intercalate (C.pack "-") [C.pack "x", C.pack "y", C.pack "z"])
  print (C.lines (C.pack "a\nbb\n\nccc"), C.unlines [C.pack "p", C.pack "q"], C.filter (/= 'l') (C.pack "hello"))
