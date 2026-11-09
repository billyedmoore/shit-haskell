module Control.Monad.ST (ST, runST, stToIO, RealWorld, stFromIO) where

import Base
import System.IO.Unsafe (unsafePerformIO)

-- ST is IO with a phantom state thread. Rank-2 `runST` is approximated (see the checker).
newtype ST s a = ST (IO a)

data RealWorld

unST :: ST s a -> IO a
unST (ST m) = m

instance Functor (ST s) where
  fmap f (ST m) = ST (fmap f m)

instance Applicative (ST s) where
  pure x = ST (pure x)
  ST f <*> ST x = ST (f <*> x)

instance Monad (ST s) where
  ST m >>= k = ST (m >>= \x -> unST (k x))

runST :: ST s a -> a
runST (ST m) = unsafePerformIO m

stToIO :: ST RealWorld a -> IO a
stToIO (ST m) = m

stFromIO :: IO a -> ST s a
stFromIO m = ST m
