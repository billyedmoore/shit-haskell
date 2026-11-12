-- Char holds surrogate code points: lone surrogates survive chr/ord, show, and the
-- roundtrip (PEP 383) encodings (os-string's encoding tests).
module Main where

import Data.Char (chr, ord, generalCategory)
import qualified GHC.Foreign as GHC
import GHC.IO.Encoding (mkTextEncoding)
import Foreign.Marshal.Array (withArrayLen, peekArray)
import Foreign.Ptr (castPtr)
import Data.Word (Word8)

main :: IO ()
main = do
  let s = [chr 0xDC95, chr 0xD800, 'a', chr 0xDFFF]
  print s
  print (map ord s, map generalCategory s)
  print (succ (chr 0xD7FF), pred (chr 0xE000), [chr 0xDBFE ..] !! 3)
  enc <- mkTextEncoding "UTF-8//ROUNDTRIP"
  let bytes = [0xDD, 0x40, 0x0D, 0xFF, 0xC3, 0xA9] :: [Word8]
  decoded <- withArrayLen bytes $ \n p -> GHC.peekCStringLen enc (castPtr p, n)
  print decoded
  back <- GHC.withCStringLen enc decoded $ \(p, n) -> peekArray n (castPtr p) :: IO [Word8]
  print (back, back == bytes)
  -- Surrogates written as literals (Cabal-syntax's `c <= '\xDFFF'`).
  let lits = ['\xD800', '\xDFFF', '\55296', '\o154000']
  print (lits, map ord lits, "a\xDC80z", length "a\xDC80z")
  print (map (\c -> c <= '\xDFFF' && c >= '\xD800') ['\xD7FF', '\xD800', '\xDBFF', '\xE000'])
  print (case chr 0xDC00 of { '\xDC00' -> "matched"; _ -> "no" })
