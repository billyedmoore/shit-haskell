{-# LANGUAGE TypeFamilies, TypeFamilyDependencies, DataKinds, KindSignatures #-}
-- A record field whose type is an injective family application over the record's parameter
-- (GHC's `hsf_vis_bndrs :: [LHsTyVarBndr () pass]`, `XRec p a = r | r -> a`), built at an
-- instance whose family reduces through another family.
module Main where

import Data.Kind (Type)

data Located l e = L l e deriving Show

type family XRec p a = r | r -> a

data Pass = Parsed | Renamed

data GhcPass (c :: Pass) = GhcPass

type family Anno a :: Type

type instance XRec (GhcPass p) a = Located (Anno a) a

data Bndr flag pass = Bndr flag String deriving Show

type instance Anno (Bndr flag (GhcPass p)) = Int

type LBndr flag pass = XRec pass (Bndr flag pass)

data Telescope pass = Vis { visAnn :: String, visBndrs :: [LBndr () pass] }

mkVis :: String -> [LBndr () (GhcPass p)] -> Telescope (GhcPass p)
mkVis an bndrs = Vis { visAnn = an, visBndrs = bndrs }

mkVis' :: String -> [LBndr () (GhcPass p)] -> Telescope (GhcPass p)
mkVis' an bndrs = Vis an bndrs

names :: Telescope (GhcPass p) -> [String]
names t = [n | L _ (Bndr () n) <- visBndrs t]

main :: IO ()
main = do
  let t = mkVis "vis" [L 1 (Bndr () "a"), L 2 (Bndr () "b")] :: Telescope (GhcPass 'Parsed)
  print (visAnn t, names t)
  putStrLn (tyName partly)
  print (names (mkVis' "v" [L 3 (Bndr () "c")] :: Telescope (GhcPass 'Renamed)))

-- A constructor partly applied where the injective family is an argument type
-- (ghc-lib-parser's `expandHsArrow (HsTyVar noAnn NotPromoted)`).
data Ty pass = TyVar String (XRec pass (Bndr () pass))

expand :: (XRec (GhcPass p) (Bndr () (GhcPass p)) -> Ty (GhcPass p)) -> Ty (GhcPass p)
expand f = f (L 9 (Bndr () "q"))

tyName :: Ty (GhcPass p) -> String
tyName (TyVar s (L n (Bndr () b))) = s ++ b ++ show n

partly :: Ty (GhcPass 'Parsed)
partly = expand (TyVar "v")
