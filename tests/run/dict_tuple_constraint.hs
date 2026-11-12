{-# LANGUAGE GADTs, ConstraintKinds, KindSignatures, RankNTypes, ScopedTypeVariables #-}
-- Matching `Dict :: c => Dict c` at a tuple of constraints gives each of them (Cabal's
-- `Dict (Binary depsRes, Show depsRes, Eq depsRes)`).
module Main where

import Data.Kind (Constraint)

data Dict (c :: Constraint) where
  Dict :: c => Dict c

data Rule where
  Rule :: forall r. Dict (Show r, Eq r, Num r) -> IO r -> Rule

runRule :: Rule -> IO String
runRule (Rule d act)
  | Dict <- d = do
      r <- act
      return (show (r + 1) ++ " " ++ show (r == r))

withDict :: Dict (Show a, Ord a) -> a -> a -> String
withDict Dict x y = show (max x y)

main :: IO ()
main = do
  runRule (Rule Dict (return (41 :: Int))) >>= putStrLn
  putStrLn (withDict Dict 'a' 'z')
  mapM_ (putStrLn . larger) [Box 3 (1 :: Int), Box "a" "b"]

-- A pattern guard over an existential, with a later guard and a where clause using the given.
data Box where
  Box :: (Show a, Ord a) => a -> a -> Box

larger :: Box -> String
larger b
  | Box x y <- b, x > y = "first " ++ shown x
  | Box _ y <- b = "second " ++ shown y
  where
    shown :: Show s => s -> String
    shown = show
