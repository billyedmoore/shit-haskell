{-# LANGUAGE OverloadedStrings #-}
import qualified Data.Text as T
import qualified Data.Text.IO as TIO
import qualified Data.Text.Encoding as TE
import qualified Data.Text.Lazy as TL
import qualified Data.Text.Lazy.Builder as TB
import qualified Data.ByteString as B
import Data.Char (toUpper)

main :: IO ()
main = do
  let t = T.pack "Hello, Wörld! ünïcode ✓"
  print t
  print (T.length t, T.toUpper t, T.words t, T.splitOn ", " t)
  print (T.replace "l" "L" t, T.reverse "abc", T.isInfixOf "Wör" t, T.breakOn "!" t)
  TIO.putStrLn (T.intercalate " | " ["a", "b", "c"])
  print (TE.encodeUtf8 "café", TE.decodeUtf8 (B.pack [0xe2, 0x9c, 0x93]))
  print (T.strip "  padded  ", T.justifyLeft 6 '.' "ab", T.center 7 '*' "x", T.count "a" "banana")
  print (T.map toUpper "shout", T.filter (/= 'a') "banana", T.foldr (:) [] "fold", T.chunksOf 3 "abcdefgh")
  print (TL.toStrict (TB.toLazyText (TB.fromText "built " <> TB.fromString (show (42 :: Int)))))
  print (read (T.unpack "123") + (1 :: Int), T.null "", T.zip "ab" "xyz", T.lines "a\nb\n", compare ("abc" :: T.Text) "abd")
