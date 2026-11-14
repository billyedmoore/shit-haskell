{-# LANGUAGE LambdaCase, GADTs, DataKinds, TypeFamilies, ScopedTypeVariables, TypeApplications, AllowAmbiguousTypes, KindSignatures, RankNTypes #-}
-- A GADT match refines a variable, so a family over it in the type of a value bound outside
-- the match reduces inside, and that value's constructors can be matched there (ghc-lib-parser's
-- `XPat ext -> case ghcPass @p of GhcRn -> case ext of HsPatExpanded ..`). The local signature's
-- `forall p` shadows the enclosing scoped `p`.
module Main where

data Pass = Parsed | Renamed

data GhcPass (p :: Pass) where
  GhcPs :: GhcPass 'Parsed
  GhcRn :: GhcPass 'Renamed

type GhcPs = GhcPass 'Parsed
type GhcRn = GhcPass 'Renamed

class IsPass (p :: Pass) where
  ghcPass :: GhcPass p

instance IsPass 'Parsed where ghcPass = GhcPs
instance IsPass 'Renamed where ghcPass = GhcRn

data Expansion a b = Expanded a b

data NoCon

type family XXPat p
type instance XXPat GhcPs = NoCon
type instance XXPat GhcRn = Expansion (Pat GhcRn) (Pat GhcRn)

data Pat pass = WildPat | VarPat String | XPat (XXPat pass)

boring :: forall p. IsPass p => Pat (GhcPass p) -> Bool
boring = go
  where
    go :: forall p. IsPass p => Pat (GhcPass p) -> Bool
    go = \case
      WildPat -> True
      VarPat _ -> True
      XPat ext -> case ghcPass @p of
        GhcRn -> case ext of
          Expanded _ pat -> go pat
        GhcPs -> False

main :: IO ()
main = do
  print (boring (WildPat :: Pat GhcPs))
  print (boring (XPat (Expanded WildPat (VarPat "x")) :: Pat GhcRn))
