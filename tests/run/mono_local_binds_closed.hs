{-# LANGUAGE GADTs, ScopedTypeVariables #-}
-- Closed local bindings stay polymorphic under MonoLocalBinds (implied by GADTs): variables a
-- binding binds itself, and type variables an annotation quantifies itself, do not make it
-- open.
module Main where

pairs :: (Char, Bool)
pairs = (pick 'a', pick True)
  where
    pick = case () of () -> \y -> y

twices :: ((Char, Char), (Bool, Bool))
twices = (twice 'a', twice True)
  where
    twice x = (id :: a -> a) (x, x)

main :: IO ()
main = do
  print pairs
  print twices
