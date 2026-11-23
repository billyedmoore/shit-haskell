{-# LANGUAGE TypeApplications, ExplicitForAll, GADTs #-}
data P where
  P :: forall {a} b. a -> b -> P
value = P @Bool 'x' True

main :: IO ()
main = case value of P _ _ -> putStrLn "ok"
