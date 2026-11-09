module Control.Concurrent (
    ThreadId, forkIO, myThreadId, threadDelay, yield, killThread, throwTo,
    module Control.Concurrent.MVar
  ) where

import Base
import Control.Concurrent.MVar
import Control.Exception (Exception(..), SomeException(..), AsyncException(..))

newtype ThreadId = ThreadId Int

instance Eq ThreadId where
  ThreadId a == ThreadId b = a == b

instance Ord ThreadId where
  compare (ThreadId a) (ThreadId b) = compare a b

instance Show ThreadId where
  showsPrec _ (ThreadId n) = showString "ThreadId " . shows n

foreign import prim "forkThread" primForkThread :: IO () -> World -> (# World, Int #)
foreign import prim "threadDelay" primThreadDelay :: Int -> World -> (# World, () #)
foreign import prim "yieldThread" primYield :: World -> (# World, () #)
foreign import prim "myThreadId" primMyThreadId :: World -> (# World, Int #)

forkIO :: IO () -> IO ThreadId
forkIO act = IO (\w -> case primForkThread act w of (# w1, n #) -> (# w1, ThreadId n #))

myThreadId :: IO ThreadId
myThreadId = IO (\w -> case primMyThreadId w of (# w1, n #) -> (# w1, ThreadId n #))

threadDelay :: Int -> IO ()
threadDelay n = IO (primThreadDelay n)

yield :: IO ()
yield = IO primYield

foreign import prim "throwTo" primThrowTo :: Int -> SomeException -> World -> (# World, () #)

throwTo :: Exception e => ThreadId -> e -> IO ()
throwTo (ThreadId n) e = IO (\w -> primThrowTo n (toException e) w)

killThread :: ThreadId -> IO ()
killThread t = throwTo t ThreadKilled
