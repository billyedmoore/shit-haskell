-- ghc-bignum's Integer API beyond arithmetic: to and from bytes, modular inverses and powers
-- (cborg's integerFromAddr, integer-gmp).
{-# LANGUAGE MagicHash, UnboxedTuples #-}
import GHC.Num.Integer
import GHC.Exts
import GHC.IO (IO (..))
import Foreign.Marshal.Alloc (allocaBytes)
import Foreign.Ptr (Ptr (..))
import Foreign.Storable (peekByteOff)
import Data.Word (Word8)

main :: IO ()
main = do
  let n = 0x0102030405060708090a :: Integer
  allocaBytes 32 $ \p@(Ptr a) -> do
    w <- integerToAddr n a 1#
    bs <- mapM (\i -> peekByteOff p i :: IO Word8) [0 .. fromIntegral w - 1]
    print (w, bs)
    back <- integerFromAddr (case w of W# x -> x) a 1#
    print (back == n)
    wl <- integerToAddr n a 0#
    backl <- integerFromAddr (case wl of W# x -> x) a 0#
    print (wl, backl == n)
    z <- integerToAddr 0 a 1#
    print z
  print (integerLog2 1024, integerBit 70, I# (integerSignum# (-5)))
  case integerPowMod# 3 200 1000000007 of
    (# r | #) -> print r
    (# | () #) -> putStrLn "none"
  case integerRecipMod# 3 7 of
    (# r | #) -> print r
    (# | () #) -> putStrLn "none"
  case integerRecipMod# 2 4 of
    (# r | #) -> print r
    (# | () #) -> putStrLn "none"
  case integerPowMod# 3 (-1) 7 of
    (# r | #) -> print r
    (# | () #) -> putStrLn "none"
