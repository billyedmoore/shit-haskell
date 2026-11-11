{-# LANGUAGE TypeFamilyDependencies, TypeFamilies, MultiParamTypeClasses, FlexibleContexts,
             DefaultSignatures, GeneralizedNewtypeDeriving, FlexibleInstances #-}
import Data.Kind (Type)
import Control.Monad.State.Strict (StateT(..), State, evalState, get, put)

class Monad m => Gen g m where
  next :: g -> m Int
  default next :: (Frozen f m, g ~ Mut f m) => g -> m Int
  next g = modifyG g (\f -> (1, f))

class Gen (Mut f m) m => Frozen f m where
  type Mut f m = (g :: Type) | g -> f
  modifyG :: Mut f m -> (f -> (a, f)) -> m a

newtype Seed = Seed Int
data SeedM = SeedM
instance Gen SeedM (State Int)
instance Frozen Seed (State Int) where
  type Mut Seed (State Int) = SeedM
  modifyG _ k = do
    n <- get
    let (a, Seed n') = k (Seed n)
    put (n' + 10)
    return a

class Step g where
  step :: g -> (Int, g)
instance Step Int where step n = (n, n + 1)
newtype Wrapped g = Wrapped g deriving (Step)

twice :: Step g => g -> (Int, Int)
twice g = let (a, Wrapped g') = step (Wrapped g); (b, _) = step g' in (a, b)

main :: IO ()
main = do
  print (evalState (next SeedM >>= \a -> get >>= \s -> return (a, s)) (5 :: Int))
  print (twice (7 :: Int))
  r <- runStateT (return 'x' :: StateT Int IO Char) 3
  print r
