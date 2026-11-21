{-# LANGUAGE TypeFamilies, DataKinds, GADTs #-}
-- Stuck given family equalities oriented by size and canonical tie-breaker
-- so equal-size pairs do not cycle and shrinking equalities always fire.

type family F a
type family G a b
type family Prod h

data Dict c where
  Dict :: c => Dict c

eqF :: (p ~ F a, p ~ F b, F a ~ G a Int) => Proxy a -> Proxy b -> Dict (F b ~ G a Int)
eqF _ _ = Dict

data Proxy a = Proxy

shrink :: (Prod (Prod h) ~ Prod h) => Proxy h -> Dict (Prod (Prod (Prod h)) ~ Prod h)
shrink _ = Dict

data Tag = A | B
type instance F Tag = Int
type instance G Tag Int = Int
type instance Prod Int = Int

main :: IO ()
main = do
  case eqF (Proxy :: Proxy Tag) (Proxy :: Proxy Tag) of
    Dict -> putStrLn "eqF ok"
  case shrink (Proxy :: Proxy Int) of
    Dict -> putStrLn "shrink ok"
