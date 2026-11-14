{-# LANGUAGE GADTs, DataKinds, TypeFamilies, TypeFamilyDependencies, ScopedTypeVariables, TypeApplications, AllowAmbiguousTypes, KindSignatures #-}
-- A class-constrained value whose type is an injective family of its variable, given to a
-- constructor whose other arguments fix the family's argument only later (ghc-lib-parser's
-- `LastStmt noExtField (noLocA return) Nothing noSyntaxExpr`): the deferred equality, through
-- injectivity, determines the variable before it is reported ambiguous.
module Main where

import Data.Kind (Type)

data Pass = Parsed | Renamed

data GhcPass (p :: Pass) where
  GhcPs :: GhcPass 'Parsed
  GhcRn :: GhcPass 'Renamed

class IsPass (p :: Pass) where
  ghcPass :: GhcPass p

instance IsPass 'Parsed where ghcPass = GhcPs
instance IsPass 'Renamed where ghcPass = GhcRn

type family SyntaxExpr p
type instance SyntaxExpr (GhcPass p) = SyntaxExprGhc p

type family SyntaxExprGhc (p :: Pass) = (r :: Type) | r -> p where
  SyntaxExprGhc 'Parsed = ()
  SyntaxExprGhc 'Renamed = String

noSyntaxExpr :: forall p. IsPass p => SyntaxExpr (GhcPass p)
noSyntaxExpr = case ghcPass @p of
  GhcPs -> ()
  GhcRn -> "none"

data Stmt idL idR body = LastStmt body (SyntaxExpr idR) | BodyStmt body

render :: forall p. IsPass p => [Stmt (GhcPass p) (GhcPass p) Int] -> [String]
render = map one
  where
    one (BodyStmt b) = show b
    one (LastStmt b _) = "last " ++ show b

stmts :: forall p. IsPass p => [Stmt (GhcPass p) (GhcPass p) Int] -> [String]
stmts ss = render (ss ++ [LastStmt 9 noSyntaxExpr])

main :: IO ()
main = do
  print (stmts [BodyStmt 1 :: Stmt (GhcPass 'Parsed) (GhcPass 'Parsed) Int])
  print (stmts ([] :: [Stmt (GhcPass 'Renamed) (GhcPass 'Renamed) Int]))
