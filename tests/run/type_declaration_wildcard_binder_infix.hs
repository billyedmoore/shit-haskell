{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
data _ :+ _ = T Int
main :: IO ()
main = case (T 4 :: Bool :+ Char) of T n -> print n
