module Data.Array.Base (
    MArray(..), IOArray, IOUArray, STArray, STUArray, UArray,
    newListArray, newArray_, readArray, writeArray, modifyArray, modifyArray',
    getElems, getAssocs, freeze, thaw, unsafeFreeze, unsafeThaw,
    runSTArray, runSTUArray
  ) where

import Base
import Data.Array
import Control.Monad.ST

-- Mutable vectors: slots hold lazy elements; writes replace the slot.
data MVec e

foreign import prim "vecNewIO" primVecNew :: Int -> e -> World -> (# World, (MVec e) #)
foreign import prim "vecReadIO" primVecRead :: MVec e -> Int -> World -> (# World, e #)
foreign import prim "vecWriteIO" primVecWrite :: MVec e -> Int -> e -> World -> (# World, () #)
foreign import prim "vecCopyIO" primVecCopy :: MVec e -> World -> (# World, (MVec e) #)

data IOArray i e = IOArray (i, i) Int (MVec e)

-- Unboxed arrays share the boxed representation; their elements are forced on write.
newtype IOUArray i e = IOUArray (IOArray i e)

newtype STArray s i e = STArray (IOArray i e)

newtype STUArray s i e = STUArray (IOArray i e)

type UArray = Array

class Monad m => MArray a e m where
  getBounds :: Ix i => a i e -> m (i, i)
  newArray :: Ix i => (i, i) -> e -> m (a i e)
  unsafeRead :: Ix i => a i e -> Int -> m e
  unsafeWrite :: Ix i => a i e -> Int -> e -> m ()

newIOArray :: Ix i => (i, i) -> e -> IO (IOArray i e)
newIOArray b x = do
  let n = rangeSize b
  v <- IO (primVecNew n x)
  return (IOArray b n v)

readIOArray :: IOArray i e -> Int -> IO e
readIOArray (IOArray _ _ v) i = IO (primVecRead v i)

writeIOArray :: IOArray i e -> Int -> e -> IO ()
writeIOArray (IOArray _ _ v) i x = IO (primVecWrite v i x)

instance MArray IOArray e IO where
  getBounds (IOArray b _ _) = return b
  newArray = newIOArray
  unsafeRead = readIOArray
  unsafeWrite = writeIOArray

instance MArray IOUArray e IO where
  getBounds (IOUArray (IOArray b _ _)) = return b
  newArray b x = fmap IOUArray (newIOArray b x)
  unsafeRead (IOUArray a) i = readIOArray a i
  unsafeWrite (IOUArray a) i x = x `seq` writeIOArray a i x

instance MArray (STArray s) e (ST s) where
  getBounds (STArray a) = stFromIO (getBounds a)
  newArray b x = stFromIO (fmap STArray (newIOArray b x))
  unsafeRead (STArray a) i = stFromIO (readIOArray a i)
  unsafeWrite (STArray a) i x = stFromIO (writeIOArray a i x)

instance MArray (STUArray s) e (ST s) where
  getBounds (STUArray (IOArray b _ _)) = return b
  newArray b x = stFromIO (fmap STUArray (newIOArray b x))
  unsafeRead (STUArray a) i = stFromIO (readIOArray a i)
  unsafeWrite (STUArray a) i x = x `seq` stFromIO (writeIOArray a i x)

newArray_ :: (MArray a e m, Ix i) => (i, i) -> m (a i e)
newArray_ b = newArray b (error "MArray: undefined array element")

newListArray :: (MArray a e m, Ix i) => (i, i) -> [e] -> m (a i e)
newListArray b xs = do
  arr <- newArray b (error "MArray: undefined array element")
  sequence_ (zipWith (unsafeWrite arr) [0 .. rangeSize b - 1] xs)
  return arr

readArray :: (MArray a e m, Ix i) => a i e -> i -> m e
readArray arr i = do
  b <- getBounds arr
  unsafeRead arr (index b i)

writeArray :: (MArray a e m, Ix i) => a i e -> i -> e -> m ()
writeArray arr i x = do
  b <- getBounds arr
  unsafeWrite arr (index b i) x

modifyArray :: (MArray a e m, Ix i) => a i e -> i -> (e -> e) -> m ()
modifyArray arr i f = do
  x <- readArray arr i
  writeArray arr i (f x)

modifyArray' :: (MArray a e m, Ix i) => a i e -> i -> (e -> e) -> m ()
modifyArray' arr i f = do
  x <- readArray arr i
  let y = f x
  y `seq` writeArray arr i y

getElems :: (MArray a e m, Ix i) => a i e -> m [e]
getElems arr = do
  b <- getBounds arr
  mapM (unsafeRead arr) [0 .. rangeSize b - 1]

getAssocs :: (MArray a e m, Ix i) => a i e -> m [(i, e)]
getAssocs arr = do
  b <- getBounds arr
  es <- getElems arr
  return (zip (range b) es)

freeze :: (MArray a e m, Ix i) => a i e -> m (Array i e)
freeze arr = do
  b <- getBounds arr
  es <- getElems arr
  return (listArray b es)

unsafeFreeze :: (MArray a e m, Ix i) => a i e -> m (Array i e)
unsafeFreeze = freeze

thaw :: (MArray a e m, Ix i) => Array i e -> m (a i e)
thaw arr = newListArray (bounds arr) (elems arr)

unsafeThaw :: (MArray a e m, Ix i) => Array i e -> m (a i e)
unsafeThaw = thaw

runSTArray :: Ix i => ST s (STArray s i e) -> Array i e
runSTArray st = runST (st >>= freeze)

runSTUArray :: Ix i => ST s (STUArray s i e) -> Array i e
runSTUArray st = runST (st >>= freeze)
