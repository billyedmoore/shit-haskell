{-# LANGUAGE TypeFamilies #-}
-- MonoLocalBinds (implied by TypeFamilies): a local function that mentions a variable of the
-- enclosing scope is not generalised, so a constraint only the enclosing body determines is
-- solved there (ghc-lib-parser's `addUnpackedness .. = HsBangTy ((o, c, noAnn), prag) ..`).
-- A closed local function is still generalised.
module Main where

class NoAnn a where noAnn :: a
instance NoAnn [b] where noAnn = []
instance NoAnn Int where noAnn = 0

type family Ext p
type instance Ext Bool = (Int, [Char])

data Node p = Node (Ext p) String

mark :: String -> Node Bool -> Node Bool
mark prag n = wrap n
  where
    wrap (Node (k, _) s) = Node (k + 1, noAnn) (s ++ prag)

pairUp :: (Int, Bool)
pairUp = (ident 3, ident True)
  where
    ident x = x

-- A local function that uses only closed siblings is closed, and generalised (GHC's
-- `steps base n ds = foldl' (step base) n ds`, used at Integer and at Int).
sums :: (Integer, Int)
sums = (steps 10 [1, 2], steps 16 [3])
  where
    steps base ds = foldl (step base) 0 ds
    step base n d = base * n + fromIntegral (d :: Int)

-- A local function using a signed sibling (with a closed signature) is closed too (GHC's
-- `hs_applicative_stmt`, used at two types, calling `hs_lstmts`).
lengths :: (Int, Int)
lengths = (count "abc", count [True])
  where
    measure :: [a] -> Int
    measure = length
    count xs = measure xs + 0

main :: IO ()
main = do
  let Node (k, a) s = mark "!" (Node (1, "x") "n")
  print (k, a, s)
  print pairUp
  print sums
  print lengths
