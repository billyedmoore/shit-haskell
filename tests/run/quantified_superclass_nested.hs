{-# LANGUAGE FlexibleContexts, StandaloneDeriving, UndecidableInstances #-}
-- Nested quantified superclasses, as bifunctors' Biff needs: Eq2 p gives
-- forall a. Eq a => Eq1 (p a), whose superclass gives forall b. Eq b => Eq (p a b).
module Main where

import Data.Functor.Classes

newtype Biff p f g a b = Biff { runBiff :: p (f a) (g b) }

deriving instance Eq (p (f a) (g b)) => Eq (Biff p f g a b)

instance (Eq2 p, Eq1 f, Eq1 g, Eq a) => Eq1 (Biff p f g a) where
  liftEq = liftEq2 (==)

instance (Eq2 p, Eq1 f, Eq1 g) => Eq2 (Biff p f g) where
  liftEq2 f g (Biff x) (Biff y) = liftEq2 (liftEq f) (liftEq g) x y

-- Uses Eq (p (f a) (g b)) from the quantified superclasses of the givens alone.
sameRun :: (Eq2 p, Eq1 f, Eq1 g, Eq a, Eq b) => Biff p f g a b -> Biff p f g a b -> Bool
sameRun x y = runBiff x == runBiff y

type B = Biff Either Maybe []

main :: IO ()
main = do
  let x = Biff (Left (Just 1)) :: B Int Char
      y = Biff (Right "ab") :: B Int Char
  print (x == x, x == y, y == Biff (Right "ab"))
  print (eq1 x x, eq1 y (Biff (Right "abc")))
  print (sameRun x x, sameRun x y)
