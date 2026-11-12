{-# LANGUAGE ConstraintKinds, FunctionalDependencies, KindSignatures, FlexibleInstances, FlexibleContexts, UndecidableSuperClasses, UndecidableInstances #-}
-- A class whose superclasses apply its constraint parameter (Cabal-syntax's
-- `class (c Token, c FilePathNT) => FieldGrammar c g | g -> c`).
module Main where

import Data.Kind (Type, Constraint)

class Pretty a where pretty :: a -> String
class Parse a where parse :: String -> a

newtype Token = Token String
instance Pretty Token where pretty (Token s) = "<" ++ s ++ ">"
instance Parse Token where parse = Token

class (c Token, c Int) => Grammar (c :: Type -> Constraint) (g :: Type -> Type) | g -> c where
  field :: c a => String -> g a

newtype Printer a = Printer (a -> String)
instance Pretty Int where pretty = show
instance Grammar Pretty Printer where
  field name = Printer (\a -> name ++ "=" ++ pretty a)

newtype Reader a = Reader (String -> a)
instance Parse Int where parse = read
instance Grammar Parse Reader where
  field _ = Reader parse

-- A class standing for two, used as the grammar's constraint: its superclasses are given.
class (Pretty a, Parse a) => Both a
instance (Pretty a, Parse a) => Both a

newtype Echo a = Echo (String -> String)
instance Grammar Both Echo where
  field name = r
    where
      r = Echo (\s -> name ++ ":" ++ pretty (parseAs r s))
      parseAs :: Parse b => Echo b -> String -> b
      parseAs _ = parse

viaSuper :: Grammar c g => g Token -> g Token
viaSuper g = g

main :: IO ()
main = do
  let Printer p = viaSuper (field "tok") :: Printer Token
      Printer q = field "n" :: Printer Int
      Reader r = field "x" :: Reader Int
  putStrLn (p (Token "a"))
  putStrLn (q 42)
  print (r "17" + 1)
  let Echo e = field "num" :: Echo Int
  putStrLn (e "5")
