{-# LANGUAGE EmptyCase, GADTs, DataKinds, TypeFamilies, ScopedTypeVariables, TypeApplications, AllowAmbiguousTypes, KindSignatures #-}
-- A `where` binding without a signature that matches on a GADT, each alternative at the type a
-- family reduces to under that alternative's refinement; the binding's type comes from its use
-- (ghc-lib-parser's `hsLinear`). Under MonoLocalBinds such a binding is not generalised.
module Main where

data Pass = Parsed | Renamed

data GhcPass (p :: Pass) where
  GhcPs :: GhcPass 'Parsed
  GhcRn :: GhcPass 'Renamed

class IsPass (p :: Pass) where
  ghcPass :: GhcPass p

instance IsPass 'Parsed where ghcPass = GhcPs
instance IsPass 'Renamed where ghcPass = GhcRn

type family XArrow (p :: Pass)
type instance XArrow 'Parsed = String
type instance XArrow 'Renamed = Int

class NoAnn a where noAnn :: a
instance NoAnn [c] where noAnn = []

data Arrow (p :: Pass) = Arrow (XArrow p)

describe :: forall p. IsPass p => Arrow p -> String
describe (Arrow x) = case ghcPass @p of
  GhcPs -> "parsed " ++ show x
  GhcRn -> "renamed " ++ show x

linear :: forall p. IsPass p => Arrow p
linear = Arrow x
  where
    x = case ghcPass @p of
      GhcPs -> noAnn
      GhcRn -> 7

-- A local function without a signature whose argument's type differs per alternative
-- (ghc-lib-parser's `pp_implicit ext = .. case ghcPass @p of GhcPs | X { .. } <- ext -> ..`).
data Info = Info { implicit :: Bool }

data Never

type family XImport (p :: Pass)
type instance XImport 'Parsed = Info
type instance XImport 'Renamed = Never

data Import (p :: Pass) = Import (XImport p) String

showImport :: forall p. IsPass p => Import p -> String
showImport (Import ext name) = name ++ flag ext
  where
    flag e = case ghcPass @p of
      GhcPs | Info { implicit = True } <- e -> " (implicit)"
            | otherwise -> ""
      GhcRn -> never e
    never :: Never -> String
    never v = case v of {}

-- A pattern binding of a GADT match, one variable with a monomorphic signature (ghc-lib-parser's
-- `(rm_fun, check_fun) = case lookup_mode of ..`).
data Mode a where
  WantNames :: Mode String
  WantSizes :: Mode Int

lookupAll :: forall a. Mode a -> [String] -> [a]
lookupAll mode xs = map check (pick xs)
  where
    pick :: [String] -> [String]
    (pick, check) = case mode of
      WantNames -> (filter (/= ""), id)
      WantSizes -> (take 2, length)

-- A local function without a signature that matches a GADT and mentions an enclosing variable
-- (GHC's `fixAssign stmt = case stmt of CmmAssign .. | reg == MachSp -> stmt`).
data O
data C

data Node e x where
  Assign :: String -> Int -> Node O O
  Branch :: Int -> Node O C

fixNode :: Int -> Node e x -> Node e x
fixNode base n = fix n
  where
    fix node = case node of
      Assign r v | r == "sp" -> node
                 | otherwise -> Assign r (v + base)
      Branch l -> Branch (l + base)

showNode :: Node e x -> String
showNode (Assign r v) = r ++ "=" ++ show v
showNode (Branch l) = "goto " ++ show l

main :: IO ()
main = do
  putStrLn (describe (linear @'Parsed))
  putStrLn (describe (linear @'Renamed))
  let y = let z = case ghcPass @'Renamed of GhcRn -> 3 in Arrow z :: Arrow 'Renamed
  putStrLn (describe y)
  putStrLn (showImport (Import (Info True) "Prelude" :: Import 'Parsed))
  putStrLn (showImport (Import (Info False) "Data.List" :: Import 'Parsed))
  print (lookupAll WantNames ["a", "", "bc"], lookupAll WantSizes ["a", "", "bc"])
  mapM_ putStrLn [showNode (fixNode 10 (Assign "sp" 1)), showNode (fixNode 10 (Assign "r1" 1)), showNode (fixNode 10 (Branch 3))]
