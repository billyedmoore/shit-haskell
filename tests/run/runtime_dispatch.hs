{-# LANGUAGE ForeignFunctionInterface, CApiFFI #-}
module Main where
import Control.Concurrent.MVar
import Control.Monad (replicateM_)
import Control.Exception
import Data.Bits
import Data.IORef
import Data.Word
import Foreign.C.Types

foreign import ccall unsafe "abs" c_abs :: CInt -> CInt
foreign import capi unsafe "stdlib.h abs" capi_abs :: CInt -> CInt

main :: IO ()
main = do
  r <- newIORef (3 :: Int)
  modifyIORef' r (\x -> x * 7 - 2)
  replicateM_ 10000 (modifyIORef' r (+ 1))
  readIORef r >>= print
  v <- newMVar (11 :: Int)
  takeMVar v >>= putMVar v . (+ 4)
  tryReadMVar v >>= print
  result <- try (evaluate (27 `quot` (0 :: Int))) :: IO (Either ArithException Int)
  print (either (const True) (const False) result)
  forced <- try (evaluate (3 + error "argument" :: Int)) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) forced)
  print (27 `quot` (4 :: Int), 27 `rem` (4 :: Int), (1.25 + 2.5) * (2 :: Double))
  print ((255 + 2 :: Word8), shiftR (128 :: Word8) 3, complement (0 :: Word16))
  print (c_abs (-17), capi_abs (-23), fromEnum 'z' - fromEnum 'a')
