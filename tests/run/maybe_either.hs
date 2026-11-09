import Data.Maybe (fromMaybe, mapMaybe, catMaybes)
import Data.Either (lefts, rights, partitionEithers)

safeDiv :: Int -> Int -> Either String Int
safeDiv _ 0 = Left "divide by zero"
safeDiv a b = Right (a `div` b)

calc :: Int -> Int -> Int -> Either String Int
calc a b c = do
  x <- safeDiv a b
  y <- safeDiv x c
  return (x + y)

main :: IO ()
main = do
  print (calc 100 5 2)
  print (calc 1 0 2)
  print (mapM (\x -> if x > 0 then Just x else Nothing) [1, 2, 3 :: Int])
  print (sequence [Just 1, Nothing :: Maybe Int])
  print (fromMaybe 0 (lookup "b" [("a", 1), ("b", 2 :: Int)]))
  print (mapMaybe (\x -> if odd x then Just (x * x) else Nothing) [1 .. 6 :: Int])
  print (catMaybes [Just 'a', Nothing, Just 'b'])
  let es = [Left "e1", Right 1, Right 2, Left "e2"] :: [Either String Int]
  print (lefts es, rights es)
  print (partitionEithers es)
  print (either length negate (Left "four" :: Either String Int))
  print (fmap (+ 1) (Right 3 :: Either String Int), (+ 1) <$> Just (4 :: Int))
