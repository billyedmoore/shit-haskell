module Control.Concurrent (
    ThreadId, forkIO, myThreadId, threadDelay, yield, killThread,
    module Control.Concurrent.MVar
  ) where

import Base
import Control.Concurrent.MVar

newtype ThreadId = ThreadId Int

instance Eq ThreadId where
  ThreadId a == ThreadId b = a == b

instance Ord ThreadId where
  compare (ThreadId a) (ThreadId b) = compare a b

instance Show ThreadId where
  showsPrec _ (ThreadId n) = showString "ThreadId " . shows n

foreign import prim "forkThread" primForkThread :: IO () -> World -> IORes Int
foreign import prim "threadDelay" primThreadDelay :: Int -> World -> IORes ()
foreign import prim "yieldThread" primYield :: World -> IORes ()
foreign import prim "myThreadId" primMyThreadId :: World -> IORes Int

forkIO :: IO () -> IO ThreadId
forkIO act = IO (\w -> case primForkThread act w of IORes n -> IORes (ThreadId n))

myThreadId :: IO ThreadId
myThreadId = IO (\w -> case primMyThreadId w of IORes n -> IORes (ThreadId n))

threadDelay :: Int -> IO ()
threadDelay n = IO (primThreadDelay n)

yield :: IO ()
yield = IO primYield

killThread :: ThreadId -> IO ()
killThread _ = return ()
