{-# LANGUAGE DeriveFoldable #-}
import Control.Exception
import Data.Int
import Data.IORef
import GHC.IORef (atomicModifyIORef'_)
import Data.Word
import System.Environment
import System.IO.Error
import System.Posix.Files (getFileStatus, isDirectory, fileExist)
import Foreign
import Foreign.C

data Rose a = Rose a [Rose a] deriving (Show, Foldable)

main :: IO ()
main = do
  as <- getArgs
  print as
  withArgs ["x", "y z"] (getArgs >>= print)
  getArgs >>= print
  n <- getProgName
  print (not (null n))
  setEnv "HSC_TEST_VAR" "value"
  lookupEnv "HSC_TEST_VAR" >>= print
  env <- getEnvironment
  print (lookup "HSC_TEST_VAR" env)
  unsetEnv "HSC_TEST_VAR"
  lookupEnv "HSC_TEST_VAR" >>= print
  r <- tryIOError (getFileStatus "no/such/file")
  print (either isDoesNotExistError (const False) r)
  s <- getFileStatus "."
  print (isDirectory s)
  fileExist "no-such-file" >>= print
  -- Sized conversions.
  print (fromIntegral (0xfffffffe :: Word32) :: Int32, fromIntegral (-2 :: Int8) :: Word8)
  print (fromIntegral (200 :: Word8) :: Int8, fromIntegral (-1 :: Int16) :: Word16)
  -- atomicModifyIORef'_ and friends.
  ref <- newIORef (1 :: Int)
  atomicModifyIORef'_ ref (* 10) >>= print
  atomicModifyIORef ref (\x -> (x + 1, x)) >>= print
  readIORef ref >>= print
  -- C strings and buffers, many times (pinned memory is reused).
  total <- sum <$> mapM (\i -> withCString (show i) (fmap fromIntegral . c_strlen)) [1 .. 3000 :: Int]
  print (total :: Int)
  print (sum (Rose 1 [Rose 2 [Rose 3 []], Rose 4 []]), length (Rose 'a' [Rose 'b' []]))

foreign import ccall unsafe "strlen" c_strlen :: CString -> IO CSize
