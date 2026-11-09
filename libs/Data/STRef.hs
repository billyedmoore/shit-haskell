module Data.STRef (STRef, newSTRef, readSTRef, writeSTRef, modifySTRef, modifySTRef') where

import Base
import Control.Monad.ST
import Data.IORef

newtype STRef s a = STRef (IORef a)

newSTRef :: a -> ST s (STRef s a)
newSTRef x = fromIO (fmap STRef (newIORef x))

readSTRef :: STRef s a -> ST s a
readSTRef (STRef r) = fromIO (readIORef r)

writeSTRef :: STRef s a -> a -> ST s ()
writeSTRef (STRef r) x = fromIO (writeIORef r x)

modifySTRef :: STRef s a -> (a -> a) -> ST s ()
modifySTRef (STRef r) f = fromIO (modifyIORef r f)

modifySTRef' :: STRef s a -> (a -> a) -> ST s ()
modifySTRef' (STRef r) f = fromIO (modifyIORef' r f)

fromIO :: IO a -> ST s a
fromIO m = stFromIO m
