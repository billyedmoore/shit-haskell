module Main where

import Data.Word
import Foreign.Marshal.Array
import GHC.Fingerprint

digest :: [Word8] -> IO Fingerprint
digest bytes = withArrayLen bytes $ \len ptr -> fingerprintData ptr len

main :: IO ()
main = do
  mapM_ (\len -> digest (take len (cycle [0 .. 255])) >>= print)
    [0, 1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1000]
  print (fingerprintString "abc")
  print (fingerprintString "\955\x1f642\x4e2d")
  first <- digest [1 .. 64]
  second <- digest [0, 255, 0, 17]
  print (fingerprintFingerprints [first, second, first])
