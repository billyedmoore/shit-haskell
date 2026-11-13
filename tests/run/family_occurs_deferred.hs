-- An equality whose variable occurs only under a type-family application (m ~ ST (PrimState m),
-- as primitive's newArray in runST gives) waits until the variable is known.
{-# LANGUAGE TypeFamilies, BangPatterns #-}
import Control.Monad.ST
import Data.STRef

class Monad m => PrimMonad m where
  type PrimState m
  liftST :: ST (PrimState m) a -> m a

instance PrimMonad (ST s) where
  type PrimState (ST s) = s
  liftST = id

newtype Box s a = Box (STRef s a)

newBox :: PrimMonad m => a -> m (Box (PrimState m) a)
newBox x = liftST (fmap Box (newSTRef x))

writeBox :: PrimMonad m => Box (PrimState m) a -> a -> m ()
writeBox (Box r) x = liftST (writeSTRef r x)

readBox :: PrimMonad m => Box (PrimState m) a -> m a
readBox (Box r) = liftST (readSTRef r)

compute :: Int -> Integer
compute len = runST $ do
    arr <- newBox 0
    if even len
    then do
        loop arr 0
    else do
        writeBox arr $! 7
        loop arr 1
  where
    loop :: Box s Integer -> Int -> ST s Integer
    loop !arr !i | i < len = do
        v <- readBox arr
        writeBox arr $! v * 10 + toInteger i
        loop arr (i + 1)
    loop arr _ = readBox arr

main :: IO ()
main = print (compute 5, compute 6)
