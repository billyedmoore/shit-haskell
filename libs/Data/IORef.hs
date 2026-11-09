module Data.IORef (IORef, newIORef, readIORef, writeIORef, modifyIORef, modifyIORef', atomicModifyIORef, atomicModifyIORef') where

import Base

data IORef a = IORef (Ref a)

foreign import prim "newRef" primNewRef :: a -> World -> (# World, (Ref a) #)
foreign import prim "readRef" primReadRef :: Ref a -> World -> (# World, a #)
foreign import prim "writeRef" primWriteRef :: Ref a -> a -> World -> (# World, () #)

newIORef :: a -> IO (IORef a)
newIORef x = IO (\w -> case primNewRef x w of (# w1, r #) -> (# w1, IORef r #))

readIORef :: IORef a -> IO a
readIORef (IORef r) = IO (primReadRef r)

writeIORef :: IORef a -> a -> IO ()
writeIORef (IORef r) x = IO (primWriteRef r x)

modifyIORef :: IORef a -> (a -> a) -> IO ()
modifyIORef ref f = readIORef ref >>= \x -> writeIORef ref (f x)

modifyIORef' :: IORef a -> (a -> a) -> IO ()
modifyIORef' ref f = readIORef ref >>= \x -> let y = f x in y `seq` writeIORef ref y

atomicModifyIORef :: IORef a -> (a -> (a, b)) -> IO b
atomicModifyIORef ref f = readIORef ref >>= \x -> let (a, b) = f x in writeIORef ref a >> return b

atomicModifyIORef' :: IORef a -> (a -> (a, b)) -> IO b
atomicModifyIORef' ref f = readIORef ref >>= \x -> case f x of (a, b) -> a `seq` (writeIORef ref a >> return b)
