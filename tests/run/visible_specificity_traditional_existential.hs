{-# LANGUAGE ExistentialQuantification, TypeApplications #-}
data G = forall {a} b. G a b
main :: IO ()
main = case G @Bool 'x' True of G _ _ -> putStrLn "ok"
