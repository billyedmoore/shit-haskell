module Control.Concurrent.MVar (MVar, newMVar, newEmptyMVar, takeMVar, putMVar, readMVar, tryTakeMVar, modifyMVar_, modifyMVar, withMVar, swapMVar) where

import Base

data MVar a
data MVarBox a = MVarBox (MVar a)

foreign import prim "newMVar" primNewMVar :: World -> (# World, (MVar a) #)
foreign import prim "takeMVar" primTakeMVar :: MVar a -> World -> (# World, a #)
foreign import prim "readMVar" primReadMVar :: MVar a -> World -> (# World, a #)
foreign import prim "putMVar" primPutMVar :: MVar a -> a -> World -> (# World, () #)
foreign import prim "tryTakeMVar" primTryTakeMVar :: MVar a -> World -> (# World, (Maybe a) #)

newEmptyMVar :: IO (MVar a)
newEmptyMVar = IO primNewMVar

newMVar :: a -> IO (MVar a)
newMVar x = newEmptyMVar >>= \m -> putMVar m x >> return m

takeMVar :: MVar a -> IO a
takeMVar m = IO (primTakeMVar m)

readMVar :: MVar a -> IO a
readMVar m = IO (primReadMVar m)

putMVar :: MVar a -> a -> IO ()
putMVar m x = IO (primPutMVar m x)

tryTakeMVar :: MVar a -> IO (Maybe a)
tryTakeMVar m = IO (primTryTakeMVar m)

modifyMVar_ :: MVar a -> (a -> IO a) -> IO ()
modifyMVar_ m f = takeMVar m >>= \x -> f x >>= \y -> putMVar m y

modifyMVar :: MVar a -> (a -> IO (a, b)) -> IO b
modifyMVar m f = takeMVar m >>= \x -> f x >>= \(y, b) -> putMVar m y >> return b

withMVar :: MVar a -> (a -> IO b) -> IO b
withMVar m f = readMVar m >>= f

swapMVar :: MVar a -> a -> IO a
swapMVar m new = takeMVar m >>= \old -> putMVar m new >> return old
