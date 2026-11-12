{-# LANGUAGE FunctionalDependencies, FlexibleInstances, UndecidableInstances, TypeFamilies #-}
{-# LANGUAGE NoMonoLocalBinds #-}
-- A pattern binding whose class constraint is solved through a functional dependency is
-- generalised over the type variables left (the monomorphism restriction does not apply to
-- them): Cabal's `tmpDir = targetDir </> makeRelativePathEx ..` used at two types.
module Main where

import Data.Kind (Type)

newtype P (a :: Type) (b :: Type) = P String deriving Show

class Join p q r | p q -> r where
  join :: p -> q -> r

instance (b ~ b', r ~ P a c) => Join (P a b) (P b' c) r where
  join (P x) (P y) = P (x ++ "/" ++ y)

mk :: String -> P a b
mk = P

atChar :: P Int Char -> String
atChar (P s) = 'c' : s

atBool :: P Int Bool -> String
atBool (P s) = 'b' : s

main :: IO ()
main = do
  let base = P "root" :: P Int ()
      sub = join base (mk "sub")
      tmp = join sub (mk "tmp")
  putStrLn (atChar tmp)
  putStrLn (atBool tmp)
  putStrLn (atChar sub)
