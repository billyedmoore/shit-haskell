{-# LANGUAGE QuantifiedConstraints, StandaloneDeriving, UndecidableInstances, TypeFamilies,
             DataKinds, TypeOperators, FlexibleContexts, TemplateHaskellQuotes, KindSignatures,
             ScopedTypeVariables, AllowAmbiguousTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy
import Language.Haskell.TH.Syntax (nameBase, nameModule)

-- Quantified constraints in instance contexts.
data Two m = Two (m Int) (m Bool)
instance (forall a. Show a => Show (m a)) => Show (Two m) where
  show (Two a b) = "Two " ++ showsPrec 11 a (' ' : showsPrec 11 b "")
deriving instance (forall x. Eq x => Eq (m x)) => Eq (Two m)

-- A given equality between a family application and a type.
type family Weight (f :: Type) :: Type where
  Weight Int = Bool
  Weight [a] = Weight a
class Describe a where describe :: Proxy a -> String
instance Describe Bool where describe _ = "Bool"
instance Describe Char where describe _ = "Char"
describeWeight :: forall a c. (Weight a ~ c, Describe c) => Proxy a -> String
describeWeight _ = describe (Proxy :: Proxy (Weight a))

-- A comprehension inside another's let.
pairs :: [(Int, [Int])]
pairs = [ (u, vs) | u <- [1, 2], let vs = [v | v <- [u, 3], v /= 2] ]

main :: IO ()
main = do
  print (Two (Just 1) (Just True))
  print (Two [1, 2] [False])
  print (Two (Just 1) Nothing == Two (Just 1) Nothing, Two [1] [] == Two [2] [])
  putStrLn (describeWeight (Proxy :: Proxy [Int]))
  print pairs
  print (nameBase ''(~), nameModule ''(~), nameBase ''(->))
