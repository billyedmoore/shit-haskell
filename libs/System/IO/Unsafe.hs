module System.IO.Unsafe (unsafePerformIO, unsafeInterleaveIO) where

import Base

foreign import prim "unsafeRunIO" primUnsafeRunIO :: IO a -> a

unsafePerformIO :: IO a -> a
unsafePerformIO m = primUnsafeRunIO m

unsafeInterleaveIO :: IO a -> IO a
unsafeInterleaveIO m = return (unsafePerformIO m)
