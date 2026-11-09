data Expr = Lit Int | Neg Expr | Expr :+: Expr | Wrap (Maybe Int) deriving (Show, Eq)
infixl 6 :+:

data Op = Plus | Minus deriving (Show, Read, Eq, Ord, Enum, Bounded)

newtype Wrapper a = Wrapper { unwrap :: a } deriving Show

main :: IO ()
main = do
  print (Lit (-3))
  print (Neg (Lit 4) :+: Lit (-5) :+: Wrap (Just (-1)))
  print (Wrapper (Just [1.5 :: Double]))
  print (Just (Left (3, 'x')) :: Maybe (Either (Int, Char) Bool))
  print (showsPrec 11 (-5 :: Int) "")
  print [Plus ..]
  print (read "Minus" :: Op)
  print (read " ( 3 , [True,False] ) " :: (Int, [Bool]))
