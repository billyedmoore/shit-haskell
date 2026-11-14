{-# LANGUAGE ApplicativeDo, ScopedTypeVariables #-}
-- ApplicativeDo: a block of independent statements ending in `pure`/`return` needs only
-- Applicative (ghc-lib-parser's `do { a' <- f a; pure (u, Entry a') }` at `Applicative f`).
module Main where

-- An Applicative that is not a Monad.
newtype Collect a = Collect ([String], a)

instance Functor Collect where
  fmap f (Collect (w, a)) = Collect (w, f a)

instance Applicative Collect where
  pure a = Collect ([], a)
  Collect (w1, f) <*> Collect (w2, a) = Collect (w1 ++ w2, f a)

note :: String -> a -> Collect a
note s a = Collect ([s], a)

pair :: Applicative f => (a -> f b) -> (Int, a) -> f (Int, b)
pair f (u, a) = do
  b <- f a
  pure (u, b)

triple :: Collect (Int, Int, String)
triple = do
  x <- note "x" 1
  y <- note "y" 2
  note "unit" ()
  z <- note "z" "three"
  return $ (x, y, z)

main :: IO ()
main = do
  let Collect (w, r) = triple
  print (w, r)
  let Collect (w2, r2) = pair (note "p") (7 :: Int, 'c')
  print (w2, r2)
  n <- do
    a <- pure 1
    b <- pure 2
    pure (a + b :: Int)
  print n
